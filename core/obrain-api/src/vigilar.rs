//! **El vigía**: enterarse de que algo cambió en tus bibliotecas mientras la
//! aplicación está abierta ([ADR-0024]).
//!
//! No indexa nada. Decide **cuándo**: tras [`PAUSA`] sin cambios que importen,
//! llama a quien lo creó, y quien lo creó vuelve a leer todo con la misma
//! llamada que «actualizar» ([`crate::indexar::reindex`]). Así no existe una
//! segunda forma de indexar que pueda discrepar de la primera.
//!
//! ```text
//!   notify ──evento──▶ ¿importa? ──sí──▶ reinicia la cuenta
//!                                         │
//!                         3 s sin nada ◀──┘──▶ al_cambiar()
//! ```
//!
//! [ADR-0024]: ../../../docs/adr/0024-vigilar-la-carpeta-en-vivo.md

use notify::event::{AccessKind, EventKind, MetadataKind, ModifyKind};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use obrain_scan::ExclusionSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Cuánto hay que esperar sin cambios antes de releer. Lo eligió el usuario
/// (ADR-0024): copiar cincuenta archivos de golpe son una o dos pasadas, y un
/// documento suelto aparece en unos cinco segundos.
pub const PAUSA: Duration = Duration::from_secs(3);

/// Terminaciones de archivos a medio escribir: el navegador descargando, un
/// programa guardando por partes. El archivo de verdad llega después, con su
/// propio evento.
const TEMPORALES: &[&str] = &[".tmp", ".crdownload", ".part", ".partial", ".download"];

// ─────────────────────────────────────────────────────────────── la pausa

/// La cuenta atrás: cada cambio que importa la reinicia, y sale cuando lleva
/// [`Pausa::espera`] sin cambios. Sin reloj propio, para poder probarla sin
/// esperar.
#[derive(Debug, Clone)]
pub struct Pausa {
    espera: Duration,
    ultimo: Option<Instant>,
}

impl Pausa {
    pub fn new(espera: Duration) -> Self {
        Pausa {
            espera,
            ultimo: None,
        }
    }

    /// Llegó un cambio que importa.
    pub fn anotar(&mut self, t: Instant) {
        self.ultimo = Some(t);
    }

    /// Cuánto falta para que salga, o `None` si no hay nada pendiente.
    pub fn falta(&self, ahora: Instant) -> Option<Duration> {
        self.ultimo
            .map(|t| (t + self.espera).saturating_duration_since(ahora))
    }

    /// ¿Ya lleva la pausa entera sin cambios? Entonces sale, y se vacía.
    pub fn salir(&mut self, ahora: Instant) -> bool {
        if self.falta(ahora) == Some(Duration::ZERO) {
            self.ultimo = None;
            true
        } else {
            false
        }
    }
}

// ─────────────────────────────────────────────────────────── qué importa

/// ¿Este evento puede cambiar lo que sabe obrain? Ante la duda, sí: releer es
/// barato y nunca pierde nada; no releer puede dejar un documento fuera.
///
/// `raices` son las bibliotecas vigiladas. Lo que se poda se mira **debajo**
/// de ellas, como hace el escáner: una bóveda dentro de `AppData` —o de una
/// carpeta que se llame `env`— se indexa, y tiene que vigilarse igual.
pub fn importa(
    evento: &notify::Result<notify::Event>,
    poda: &ExclusionSet,
    raices: &[PathBuf],
) -> bool {
    let Ok(e) = evento else {
        // Un error del vigía (la cola del sistema desbordada, una carpeta que
        // desapareció): no se sabe qué cambió, así que se relee.
        return true;
    };
    match e.kind {
        // Leer no cambia nada, y el propio indexado lee: sin esto se
        // despertaría a sí mismo.
        EventKind::Access(AccessKind::Read | AccessKind::Open(_) | AccessKind::Close(_))
        | EventKind::Access(AccessKind::Any | AccessKind::Other) => return false,
        EventKind::Modify(ModifyKind::Metadata(MetadataKind::AccessTime)) => return false,
        // En Windows, crear o borrar algo dentro de una carpeta le cambia la
        // fecha a la carpeta, y llega como `Modify(Any)` sobre ella. Lo que
        // cambió de verdad llega en su propio evento, con su ruta: contar el de
        // la carpeta hacía que el candado de Word despertara al vigía.
        EventKind::Modify(ModifyKind::Any)
            if !e.paths.is_empty() && e.paths.iter().all(|p| p.is_dir()) =>
        {
            return false;
        }
        _ => {}
    }
    // Sin ruta no se sabe dónde: cuenta.
    e.paths.is_empty()
        || e.paths.iter().any(|p| {
            let debajo = raices
                .iter()
                .find_map(|r| p.strip_prefix(r).ok())
                .unwrap_or(p);
            ruta_importa(debajo, poda)
        })
}

/// ¿Un cambio en esta ruta puede cambiar la bóveda? No, si es un temporal o
/// ruido que el escáner ignora, o si cuelga de una carpeta que el escáner poda.
pub fn ruta_importa(ruta: &Path, poda: &ExclusionSet) -> bool {
    if poda.path_is_excluded(ruta).is_some() {
        return false;
    }
    let Some(nombre) = ruta.file_name().map(|n| n.to_string_lossy()) else {
        return true;
    };
    let minusculas = nombre.to_lowercase();
    // El candado de Word y Excel mientras un documento está abierto.
    if nombre.starts_with("~$") {
        return false;
    }
    if TEMPORALES.iter().any(|t| minusculas.ends_with(t)) {
        return false;
    }
    poda.skip_file(&nombre, 0).is_none()
}

// ─────────────────────────────────────────────────────────────── el vigía

/// Vigila las bibliotecas mientras exista. Soltarlo lo para: se cierra el
/// canal y el hilo termina.
pub struct Vigia {
    watcher: Mutex<RecommendedWatcher>,
    /// Compartidas con el hilo, que las necesita para saber qué parte de la
    /// ruta de un evento cuelga de una biblioteca.
    raices: Arc<Mutex<Vec<PathBuf>>>,
}

impl Vigia {
    /// Empieza sin vigilar nada; [`Vigia::sincronizar`] le dice qué.
    ///
    /// `al_cambiar` se llama desde un hilo propio del vigía, una vez por cada
    /// racha de cambios, cuando lleva `espera` sin llegar ninguno.
    pub fn empezar(
        espera: Duration,
        mut al_cambiar: impl FnMut() + Send + 'static,
    ) -> Result<Self, notify::Error> {
        let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
        let watcher = notify::recommended_watcher(tx)?;
        let raices: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
        let del_hilo = Arc::clone(&raices);
        std::thread::Builder::new()
            .name("obrain-vigia".into())
            .spawn(move || {
                let poda = ExclusionSet::new();
                let mut pausa = Pausa::new(espera);
                loop {
                    let evento = match pausa.falta(Instant::now()) {
                        None => match rx.recv() {
                            Ok(e) => Some(e),
                            Err(_) => return, // el vigía se soltó
                        },
                        Some(d) => match rx.recv_timeout(d) {
                            Ok(e) => Some(e),
                            Err(RecvTimeoutError::Timeout) => None,
                            Err(RecvTimeoutError::Disconnected) => return,
                        },
                    };
                    if let Some(e) = evento {
                        let vigiladas = del_hilo.lock().map(|r| r.clone()).unwrap_or_default();
                        if importa(&e, &poda, &vigiladas) {
                            pausa.anotar(Instant::now());
                        }
                    }
                    if pausa.salir(Instant::now()) {
                        al_cambiar();
                    }
                }
            })
            .map_err(|e| notify::Error::generic(&e.to_string()))?;
        Ok(Vigia {
            watcher: Mutex::new(watcher),
            raices,
        })
    }

    /// Vigila exactamente estas carpetas: empieza con las nuevas y deja las
    /// que ya no están. Una que no existe en el disco (un USB desconectado) se
    /// salta sin error: `indexar` ya la cuenta como ausente.
    pub fn sincronizar(&self, raices: &[PathBuf]) {
        let (Ok(mut w), Ok(mut vigiladas)) = (self.watcher.lock(), self.raices.lock()) else {
            return;
        };
        vigiladas.retain(|r| {
            let sigue = raices.contains(r) && r.is_dir();
            if !sigue {
                let _ = w.unwatch(r);
            }
            sigue
        });
        for r in raices {
            if !vigiladas.contains(r) && r.is_dir() && w.watch(r, RecursiveMode::Recursive).is_ok()
            {
                vigiladas.push(r.clone());
            }
        }
    }

    /// Las carpetas que está vigilando ahora.
    pub fn raices(&self) -> Vec<PathBuf> {
        self.raices.lock().map(|r| r.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, DataChange};

    fn ev(kind: EventKind, ruta: &str) -> notify::Result<notify::Event> {
        Ok(notify::Event::new(kind).add_path(PathBuf::from(ruta)))
    }

    #[test]
    fn la_pausa_sale_una_vez_tras_la_calma() {
        let t0 = Instant::now();
        let s = Duration::from_secs(1);
        let mut p = Pausa::new(Duration::from_secs(3));
        assert_eq!(p.falta(t0), None, "sin cambios no hay nada pendiente");
        assert!(!p.salir(t0 + 10 * s));

        // Una racha: cada cambio reinicia la cuenta.
        p.anotar(t0);
        p.anotar(t0 + 2 * s);
        assert!(!p.salir(t0 + 4 * s), "a los 2 s del último, todavía no");
        assert_eq!(p.falta(t0 + 4 * s), Some(s));
        assert!(p.salir(t0 + 5 * s), "a los 3 s del último, sí");
        assert!(!p.salir(t0 + 6 * s), "y sólo una vez");
        assert_eq!(p.falta(t0 + 6 * s), None);
    }

    #[test]
    fn leer_no_despierta_al_vigia() {
        let poda = ExclusionSet::new();
        let doc = "/boveda/01 Identidad/IMG_2291.jpg";
        assert!(!importa(
            &ev(EventKind::Access(AccessKind::Read), doc),
            &poda,
            &[]
        ));
        assert!(!importa(
            &ev(
                EventKind::Access(AccessKind::Open(notify::event::AccessMode::Read)),
                doc
            ),
            &poda,
            &[]
        ));
        assert!(!importa(
            &ev(
                EventKind::Modify(ModifyKind::Metadata(MetadataKind::AccessTime)),
                doc
            ),
            &poda,
            &[]
        ));
        assert!(importa(
            &ev(EventKind::Create(CreateKind::File), doc),
            &poda,
            &[]
        ));
        assert!(importa(
            &ev(EventKind::Modify(ModifyKind::Data(DataChange::Any)), doc),
            &poda,
            &[]
        ));
        assert!(importa(
            &ev(EventKind::Remove(notify::event::RemoveKind::Any), doc),
            &poda,
            &[]
        ));
    }

    #[test]
    fn los_temporales_y_el_ruido_no_importan() {
        let poda = ExclusionSet::new();
        for ruta in [
            "/boveda/05 Vivienda/~$ntrato de alquiler.docx",
            "/boveda/Descargas/poliza.pdf.crdownload",
            "/boveda/Descargas/foto.jpg.part",
            "/boveda/02 Salud/informe.TMP",
            "/boveda/01 Identidad/Thumbs.db",
            "/boveda/01 Identidad/desktop.ini",
            "/boveda/proyecto/node_modules/x/index.js",
            "/boveda/proyecto/.git/index",
        ] {
            assert!(!ruta_importa(Path::new(ruta), &poda), "{ruta}");
        }
        for ruta in [
            "/boveda/05 Vivienda/Contrato de alquiler.docx",
            "/boveda/01 Identidad",
            "/boveda/02 Salud/2026-09-01 Póliza.pdf",
        ] {
            assert!(ruta_importa(Path::new(ruta), &poda), "{ruta}");
        }
    }

    /// Lo que hay por encima de la biblioteca no se poda: una bóveda dentro de
    /// `AppData` se indexa, y se vigila igual. Lo de dentro sí se poda.
    #[test]
    fn se_poda_por_debajo_de_la_biblioteca_no_por_encima() {
        let poda = ExclusionSet::new();
        let raiz = PathBuf::from("/home/jp/AppData/Local/Temp/Boveda");
        let cambio = |ruta: &str| ev(EventKind::Create(CreateKind::File), ruta);
        assert!(importa(
            &cambio("/home/jp/AppData/Local/Temp/Boveda/01 Identidad/tie.jpg"),
            &poda,
            std::slice::from_ref(&raiz)
        ));
        assert!(!importa(
            &cambio("/home/jp/AppData/Local/Temp/Boveda/app/node_modules/x.js"),
            &poda,
            std::slice::from_ref(&raiz)
        ));
    }

    #[test]
    fn ante_la_duda_se_relee() {
        let poda = ExclusionSet::new();
        assert!(importa(
            &Err(notify::Error::generic("cola desbordada")),
            &poda,
            &[]
        ));
        assert!(importa(&Ok(notify::Event::new(EventKind::Any)), &poda, &[]));
    }
}
