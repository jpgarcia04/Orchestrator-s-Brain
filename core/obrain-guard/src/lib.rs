//! # obrain-guard — sensibilidad y egreso · invariante I11
//!
//! No es un hito del roadmap: es la **puerta que M4 tiene que atravesar**.
//!
//! El informe de M2 lo dejó escrito con nombres y apellidos: *«M2 es el hito en
//! el que el contenido sale del archivo, y la sensibilidad todavía no existe.
//! Todas las entidades son `normal`. En el índice han entrado ficheros `.env`
//! con variables de credenciales y hay un archivo llamado `VPS password.txt`»*.
//! Mientras el único índice era léxico y local no se violaba nada —§5.2 concede
//! BM25 local incluso a `vault`—. En cuanto exista un índice vectorial, sí.
//!
//! ```text
//!   scan  →  extract  →  link  →  GUARD  →  embed (M4)
//!                                   ↑
//!                        aqui se decide que puede
//!                        vectorizarse y que puede salir
//! ```
//!
//! ## Dos mitades que se necesitan
//!
//! - [`rules`] mira **el nombre**. Es lo único que funciona sobre lo que no se
//!   puede leer: un `.pfx` binario, un pasaporte escaneado sin OCR, los 14
//!   documentos de Office deshidratados en OneDrive.
//! - [`content`] mira **el texto que M2 ya extrajo**. No abre archivos: lee la
//!   tabla `chunks`. Es lo que caza lo que ningún nombre delata — un `notas.md`
//!   con una clave de API dentro.
//!
//! ## Y una decisión que no es obvia
//!
//! El nivel se escribe **dos veces**: como aserción `sensitivity_level` en el
//! log —procedencia (I9) y canal de corrección (I5)— y como columna
//! `entities.sensitivity` —el punto de control que lee [`gate`]—. La columna es
//! una proyección de las aserciones y se reconstruye entera con
//! `Store::reproject_sensitivity`. El razonamiento largo, en
//! [`docs/adr/0006`](../../../docs/adr/0006-donde-vive-la-sensibilidad.md).

pub mod content;
pub mod gate;
pub mod rules;

use obrain_core::rusqlite::Connection;
use obrain_core::{
    Assertion, AssertionId, EntityId, Error, Method, NewAssertion, Origin, Result, Sensitivity,
    Store, TimePoint, Value, CONTENTS_SENSITIVITY,
};
use obrain_extract::FileRow;
use rules::Signal;
use std::collections::{BTreeMap, HashMap};

pub use gate::{Capability, Decision, Runs};

/// Identidad del clasificador. Va en `source` de cada aserción: cambiar las
/// reglas y no cambiar esto haría imposible saber qué versión escribió qué.
pub const CLASSIFIER: &str = "guard";
pub const CLASSIFIER_VERSION: i64 = 1;

/// Lo que el clasificador concluyó sobre un archivo.
#[derive(Debug, Clone)]
pub struct Verdict {
    pub entity: EntityId,
    pub path: String,
    pub signal: Signal,
    /// Ordinal del fragmento donde apareció la señal, si vino del contenido.
    pub chunk: Option<i64>,
}

impl Verdict {
    /// Dónde mirar para comprobarlo — **nunca qué se encontró**.
    ///
    /// Copiar aquí la clave de API recién detectada la habría metido en un log
    /// append-only e inmutable por I1/I4, es decir, para siempre. La evidencia
    /// es una dirección, no una cita.
    pub fn evidence(&self) -> String {
        match self.chunk {
            Some(o) => format!("regla={} fragmento={o}", self.signal.rule),
            None => format!("regla={}", self.signal.rule),
        }
    }

    pub fn source(&self) -> String {
        let origen = if self.chunk.is_some() { "content" } else { "name" };
        format!("{CLASSIFIER}:{origen}@{CLASSIFIER_VERSION}")
    }
}

// ──────────────────────────────────────────────────────────────────── informe

#[derive(Debug, Default)]
pub struct GuardReport {
    /// Archivos del log que se han mirado.
    pub considered: usize,
    /// De ellos, cuántos tenían texto extraído que escanear.
    pub with_text: usize,

    pub normal: usize,
    pub sensitive: usize,
    pub vault: usize,

    /// Cuántos veredictos salieron del nombre y cuántos del contenido.
    pub from_name: usize,
    pub from_content: usize,
    pub by_rule: BTreeMap<&'static str, usize>,

    pub assertions_written: usize,
    pub assertions_retracted: usize,
    pub assertions_unchanged: usize,
    /// Entidades donde el usuario se pronunció y el clasificador no manda.
    pub user_overrides: usize,
    /// Archivos que suben de nivel por estar dentro de una carpeta que el
    /// usuario marcó ([ADR-0020]). Es la señal que rescata a la foto de la TIE,
    /// que no tiene ni nombre útil ni texto.
    ///
    /// [ADR-0020]: ../../../docs/adr/0020-sensibilidad-por-carpeta.md
    pub inherited: usize,
    /// El nivel **efectivo** de los archivos mirados, después de aplicar al
    /// usuario y a las carpetas: `[normal, sensitive, vault]`. Los tres campos
    /// de arriba cuentan lo que dijo el clasificador; éste, lo que lee la
    /// puerta.
    pub effective: [usize; 3],

    /// Coste real de la política: fragmentos que M4 no podrá vectorizar.
    pub withheld_chunks: i64,
    /// Censo de la tabla `entities` entera, contenedores incluidos.
    pub census: [i64; 3],
}

impl GuardReport {
    /// Todo archivo mirado acaba exactamente en un nivel.
    pub fn arithmetic_holds(&self) -> bool {
        self.normal + self.sensitive + self.vault == self.considered
            && self.from_name + self.from_content == self.sensitive + self.vault
    }
}

// ─────────────────────────────────────────────────────────────────── pipeline

/// Clasifica todo el corpus y proyecta la columna. El llamante envuelve en
/// transacción.
pub fn classify_all(
    store: &Store,
    files: &[FileRow],
    at: i64,
    dry_run: bool,
    report: &mut GuardReport,
) -> Result<Vec<Verdict>> {
    let tp = TimePoint::at(at);
    report.considered = files.len();

    // Una consulta para todo el contenido, no una por archivo.
    let por_contenido = scan_chunks(store.conn(), report)?;
    // Y una para lo que ya estaba dicho, para no reafirmar lo idéntico.
    let previas = previous_claims(store, tp)?;
    // Las carpetas que marcó el usuario, una vez: son la herencia (ADR-0020).
    let marcas = store.folder_marks(tp)?;

    let mut verdicts = Vec::new();
    for file in files {
        let name = file.file_name();
        let por_nombre = rules::classify_name(name);
        let del_padre = parent_dir(&file.path).and_then(rules::classify_parent);
        let del_texto = por_contenido.get(&file.entity).copied();

        // Gana la más restrictiva; a igual nivel, la de más confianza.
        let ganadora = [
            del_texto.map(|(s, o)| (s, Some(o))),
            por_nombre.map(|s| (s, None)),
            del_padre.map(|s| (s, None)),
        ]
        .into_iter()
        .flatten()
        .max_by(|(a, _), (b, _)| {
            a.level
                .cmp(&b.level)
                .then(a.confidence.total_cmp(&b.confidence))
        });

        let Some((signal, chunk)) = ganadora else {
            report.normal += 1;
            // Si antes había una señal y ahora no, se retracta: la proyección
            // tiene que seguir a las reglas de hoy.
            if !dry_run {
                retract_stale(store, &previas, &file.entity, None, at, report)?;
                project(store, &file.entity, tp, &marcas, None, report)?;
            }
            continue;
        };

        match signal.level {
            Sensitivity::Vault => report.vault += 1,
            Sensitivity::Sensitive => report.sensitive += 1,
            Sensitivity::Normal => report.normal += 1,
        }
        if chunk.is_some() {
            report.from_content += 1;
        } else {
            report.from_name += 1;
        }
        *report.by_rule.entry(signal.rule).or_default() += 1;

        let verdict = Verdict {
            entity: file.entity.clone(),
            path: file.path.clone(),
            signal,
            chunk,
        };

        if !dry_run {
            write_claim(store, &previas, &verdict, at, report)?;
            project(store, &file.entity, tp, &marcas, Some(signal.level), report)?;
        }
        verdicts.push(verdict);
    }

    // Las carpetas no tienen texto ni pasan por las reglas, pero sí heredan: una
    // subcarpeta que aparece dentro de una carpeta marcada tiene que enseñar su
    // candado igual que sus archivos. Sólo se escribe lo que cambia.
    if !dry_run {
        for a in store.query(None, Some("folder_path"), tp)? {
            let nivel = store.resolve_sensitivity(&a.subject, tp, &marcas)?.level;
            if store.sensitivity(&a.subject)? != nivel {
                store.set_sensitivity(&a.subject, nivel)?;
            }
        }
    }

    report.withheld_chunks = gate::withheld_chunks(store.conn())?;
    report.census = store.sensitivity_census()?;
    Ok(verdicts)
}

/// Escribe el nivel efectivo de un archivo y cuenta **por qué** es ése.
///
/// Es [`Store::reproject_sensitivity_under`] desplegada: hace lo mismo, pero
/// quiere saber de dónde salió el nivel para que el informe distinga lo que
/// dijo el clasificador, lo que corrigió el usuario y lo que heredó de una
/// carpeta.
fn project(
    store: &Store,
    entity: &EntityId,
    tp: TimePoint,
    marcas: &obrain_core::FolderMarks,
    clasificador: Option<Sensitivity>,
    report: &mut GuardReport,
) -> Result<()> {
    let r = store.resolve_sensitivity(entity, tp, marcas)?;
    store.set_sensitivity(entity, r.level)?;
    let dijo = clasificador.unwrap_or(Sensitivity::Normal);
    match r.origin {
        // La columna no coincide con lo que dijo el clasificador: el usuario
        // mandó otra cosa (I5).
        Origin::User if clasificador.is_some() && r.level != dijo => report.user_overrides += 1,
        Origin::Folder(_) if r.level > dijo => report.inherited += 1,
        _ => {}
    }
    report.effective[r.level as usize] += 1;
    Ok(())
}

/// Escanea el texto ya indexado. Devuelve, por entidad, la señal más fuerte y
/// el ordinal del fragmento donde apareció.
fn scan_chunks(
    conn: &Connection,
    report: &mut GuardReport,
) -> Result<HashMap<EntityId, (Signal, i64)>> {
    let mut stmt = conn.prepare("SELECT entity, ordinal, text FROM chunks ORDER BY entity, ordinal")?;
    let mut rows = stmt.query([])?;
    let mut mejor: HashMap<EntityId, (Signal, i64)> = HashMap::new();
    let mut vistas: std::collections::HashSet<EntityId> = std::collections::HashSet::new();

    while let Some(row) = rows.next()? {
        let entity = EntityId(row.get(0)?);
        let ordinal: i64 = row.get(1)?;
        vistas.insert(entity.clone());

        // Una entidad ya marcada como bóveda no puede subir de nivel.
        if mejor.get(&entity).is_some_and(|(s, _)| s.level == Sensitivity::Vault) {
            continue;
        }
        let texto: String = row.get(2)?;
        if let Some(s) = content::scan(&texto) {
            let sube = mejor
                .get(&entity)
                .is_none_or(|(p, _)| (s.level, s.confidence) > (p.level, p.confidence));
            if sube {
                mejor.insert(entity, (s, ordinal));
            }
        }
    }
    report.with_text = vistas.len();
    Ok(mejor)
}

/// Las aserciones `sensitivity_level` vigentes, agrupadas por entidad.
fn previous_claims(store: &Store, tp: TimePoint) -> Result<HashMap<EntityId, Vec<Assertion>>> {
    let mut m: HashMap<EntityId, Vec<Assertion>> = HashMap::new();
    for a in store.query(None, Some("sensitivity_level"), tp)? {
        m.entry(a.subject.clone()).or_default().push(a);
    }
    Ok(m)
}

/// Escribe la afirmación del clasificador, si dice algo nuevo.
fn write_claim(
    store: &Store,
    previas: &HashMap<EntityId, Vec<Assertion>>,
    v: &Verdict,
    at: i64,
    report: &mut GuardReport,
) -> Result<()> {
    let nivel = v.signal.level;
    let ya_dicho = previas.get(&v.entity).into_iter().flatten().any(|a| {
        a.method != Method::User
            && a.object.as_text() == Some(nivel.as_str())
            && a.evidence.as_deref() == Some(v.evidence().as_str())
    });
    if ya_dicho {
        report.assertions_unchanged += 1;
        return Ok(());
    }

    retract_stale(store, previas, &v.entity, Some(nivel), at, report)?;
    store.assert(
        NewAssertion::new(
            v.entity.clone(),
            "sensitivity_level",
            Value::Text(nivel.as_str().to_string()),
            Method::Inferred,
            at,
        )
        .source(&v.source())
        .confidence(v.signal.confidence)
        .evidence(&v.evidence()),
    )?;
    report.assertions_written += 1;
    Ok(())
}

/// Retracta lo que el clasificador dijo antes y ya no sostiene.
///
/// **Nunca toca una aserción del usuario.** Que el clasificador pudiera
/// retractar una corrección manual convertiría I5 —«las aserciones del usuario
/// tienen precedencia permanente»— en una promesa que dura hasta el siguiente
/// reescaneo, que es la forma más rápida de perder la confianza para siempre.
fn retract_stale(
    store: &Store,
    previas: &HashMap<EntityId, Vec<Assertion>>,
    entity: &EntityId,
    ahora: Option<Sensitivity>,
    at: i64,
    report: &mut GuardReport,
) -> Result<()> {
    let obsoletas: Vec<&AssertionId> = previas
        .get(entity)
        .into_iter()
        .flatten()
        .filter(|a| a.method != Method::User && a.source.starts_with(CLASSIFIER))
        .filter(|a| ahora.is_none_or(|n| a.object.as_text() != Some(n.as_str())))
        .map(|a| &a.id)
        .collect();
    for id in obsoletas {
        store.retract(id, at)?;
        report.assertions_retracted += 1;
    }
    Ok(())
}

/// Nombre de la carpeta que contiene al archivo, sin la ruta completa.
fn parent_dir(path: &str) -> Option<&str> {
    let (dir, _) = path.rsplit_once(['\\', '/'])?;
    dir.rsplit(['\\', '/']).next()
}

/// Marca un archivo a mano. Es el canal de corrección del usuario (I5) y la
/// operación que la barra de comandos expondrá como «esto es privado».
pub fn mark_by_user(store: &Store, entity: &EntityId, level: Sensitivity, at: i64) -> Result<()> {
    store.assert(
        NewAssertion::new(
            entity.clone(),
            "sensitivity_level",
            Value::Text(level.as_str().to_string()),
            Method::User,
            at,
        )
        .source("user:manual"),
    )?;
    store.reproject_sensitivity(entity, TimePoint::at(at))?;
    Ok(())
}

// ─────────────────────────────────────────────────────────── carpetas (0020)

/// Lo que pasó al marcar o desmarcar una carpeta.
#[derive(Debug, Default, Clone)]
pub struct FolderMarkReport {
    /// Entidades dentro de la carpeta —archivos y subcarpetas, ella incluida—
    /// cuyo nivel se ha vuelto a calcular.
    pub under: usize,
    /// De ellas, las que han cambiado de nivel.
    pub changed: usize,
    /// El nivel de la marca que queda puesta, o `None` si se quitó.
    pub level: Option<Sensitivity>,
}

/// **Marca una carpeta** —todo lo que tiene dentro hereda el nivel, también lo
/// que llegue después— o, con `None`, le quita la marca.
///
/// Es una aserción del usuario (I5) sobre la carpeta, así que ningún reescaneo
/// la pisa: el clasificador sólo retracta lo que escribió él. Quitarla sí la
/// retracta, y es el usuario quien lo hace: la marca queda en el log con su
/// fecha de alta y de baja, que es lo que responde *«¿qué protegía yo en
/// marzo?»* (I4, I6).
///
/// Sólo sube: `normal` no es una marca. Para que un archivo concreto de una
/// carpeta marcada deje de ser sensible, se corrige **ese archivo**
/// ([`mark_by_user`]), y como es posterior a la marca, gana ([ADR-0020]).
///
/// El llamante envuelve en transacción: son varias escrituras y la columna no
/// puede quedarse a medias.
///
/// [ADR-0020]: ../../../docs/adr/0020-sensibilidad-por-carpeta.md
pub fn mark_folder(
    store: &Store,
    folder: &EntityId,
    level: Option<Sensitivity>,
    at: i64,
) -> Result<FolderMarkReport> {
    let tp = TimePoint::at(at);
    let ruta = store
        .resolve(folder, "folder_path", tp)?
        .and_then(|a| a.object.as_text().map(str::to_string))
        .ok_or_else(|| Error::NotAFolder(folder.to_string()))?;
    if level == Some(Sensitivity::Normal) {
        return Err(Error::InvalidMark(Sensitivity::Normal.as_str().to_string()));
    }

    // Cardinalidad `one`: la marca anterior, si la hay, deja de estar vigente
    // antes de poner la nueva. Sólo las del usuario, que son las únicas que
    // cuentan.
    let vigentes: Vec<AssertionId> = store
        .query(Some(folder), Some(CONTENTS_SENSITIVITY), tp)?
        .into_iter()
        .filter(|a| a.method == Method::User)
        .map(|a| a.id)
        .collect();
    let misma = level.is_some_and(|l| {
        store
            .resolve(folder, CONTENTS_SENSITIVITY, tp)
            .ok()
            .flatten()
            .is_some_and(|a| a.method == Method::User && a.object.as_text() == Some(l.as_str()))
    });
    if !misma {
        for id in &vigentes {
            store.retract(id, at)?;
        }
        if let Some(l) = level {
            store.assert(
                NewAssertion::new(
                    folder.clone(),
                    CONTENTS_SENSITIVITY,
                    Value::Text(l.as_str().to_string()),
                    Method::User,
                    at,
                )
                .source("user:carpeta"),
            )?;
        }
    }

    // Todo lo que está **ahora** debajo de su ruta, a cualquier profundidad:
    // archivos por su ruta observada, subcarpetas por la suya.
    let marcas = store.folder_marks(tp)?;
    let mut report = FolderMarkReport { level, ..Default::default() };
    let mut vistos = std::collections::HashSet::new();
    for pred in ["path_observed_at", "folder_path"] {
        for a in store.query(None, Some(pred), tp)? {
            if !vistos.insert(a.subject.clone()) {
                continue;
            }
            // La ruta vigente, no una antigua: `query` trae todas.
            let Some(donde) = store.location(&a.subject, tp)? else { continue };
            if !obrain_core::sensitivity::path_within(&donde, &ruta) {
                continue;
            }
            let antes = store.sensitivity(&a.subject)?;
            let ahora = store.reproject_sensitivity_under(&a.subject, tp, &marcas)?;
            report.under += 1;
            if antes != ahora {
                report.changed += 1;
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_padre_es_solo_la_carpeta_inmediata() {
        assert_eq!(parent_dir(r"C:\Users\a\Visados\solicitud.pdf"), Some("Visados"));
        assert_eq!(parent_dir("suelto.txt"), None);
    }

    /// I5 en su forma más incómoda: el usuario puede **bajar** el nivel, y el
    /// clasificador no puede devolverlo a su sitio en la siguiente pasada.
    #[test]
    fn el_usuario_gana_incluso_para_bajar_el_nivel() {
        let store = Store::open_in_memory().unwrap();
        let at = 1_000;
        let tp = TimePoint::at(at);
        let e = store.create_entity(at).unwrap();

        store
            .assert(
                NewAssertion::new(e.clone(), "sensitivity_level", "vault", Method::Inferred, at)
                    .source("guard:name@1"),
            )
            .unwrap();
        assert_eq!(store.reproject_sensitivity(&e, tp).unwrap(), Sensitivity::Vault);

        mark_by_user(&store, &e, Sensitivity::Normal, at + 1).unwrap();
        assert_eq!(store.sensitivity(&e).unwrap(), Sensitivity::Normal);

        // Y una reproyección posterior no lo revierte.
        let despues = TimePoint::at(at + 2);
        assert_eq!(store.reproject_sensitivity(&e, despues).unwrap(), Sensitivity::Normal);
    }

    /// Sin usuario de por medio, entre dos señales gana la que no filtra.
    #[test]
    fn sin_usuario_gana_la_senal_mas_restrictiva() {
        let store = Store::open_in_memory().unwrap();
        let at = 1_000;
        let tp = TimePoint::at(at);
        let e = store.create_entity(at).unwrap();

        for nivel in ["sensitive", "vault"] {
            store
                .assert(
                    NewAssertion::new(e.clone(), "sensitivity_level", nivel, Method::Inferred, at)
                        .source("guard:name@1"),
                )
                .unwrap();
        }
        assert_eq!(store.reproject_sensitivity(&e, tp).unwrap(), Sensitivity::Vault);
    }

    #[test]
    fn sin_ninguna_senal_el_nivel_es_normal() {
        let store = Store::open_in_memory().unwrap();
        let at = 1_000;
        let e = store.create_entity(at).unwrap();
        assert_eq!(
            store.reproject_sensitivity(&e, TimePoint::at(at)).unwrap(),
            Sensitivity::Normal
        );
    }
}
