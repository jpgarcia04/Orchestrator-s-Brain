//! La sensibilidad **efectiva** de una entidad: lo que se afirmó de ella, y lo
//! que hereda de una carpeta que el usuario marcó.
//!
//! [ADR-0006] dejó la sensibilidad en dos sitios —la columna que lee la puerta
//! y las aserciones que la explican— con un único puente entre ambos:
//! [`Store::reproject_sensitivity`]. [ADR-0020] le añade una segunda fuente, y
//! por eso vive aquí y no en el clasificador: si la herencia se calculara sólo
//! al clasificar, cualquier otra reproyección (una corrección a mano, un test)
//! la tiraría sin avisar.
//!
//! ## La marca de carpeta
//!
//! `contents_sensitivity` sobre una carpeta, **siempre del usuario** (I5):
//! *«todo lo que esté aquí dentro es, como poco, sensible»*. Tres propiedades,
//! y las tres son la decisión:
//!
//! 1. **Es un suelo, no un nivel.** Sube lo de dentro y nunca lo baja: un
//!    `.env` de bóveda dentro de una carpeta sensible sigue siendo bóveda.
//! 2. **Cubre por ruta, no por aristas.** Lo de dentro es lo que *ahora* está
//!    debajo de la ruta de la carpeta, a cualquier profundidad. Así hereda lo
//!    que llegue después sin que nadie lo reescriba, y no depende de que M3 haya
//!    retractado el `contained_by` viejo de un archivo que se movió.
//! 3. **Gana la última palabra del usuario.** Si marcaste un archivo como
//!    normal *después* de marcar su carpeta, es una excepción consciente y se
//!    respeta. Si lo marcaste *antes*, la carpeta es lo último que dijiste.
//!
//! ## Lo que lleva dentro
//!
//! [ADR-0030] añade la tercera fuente: **un documento es, como poco, tan
//! sensible como lo más sensible que lleva dentro** (`part_of`). El PDF de la
//! visa que junta la carátula del pasaporte no puede salir sin tapar porque el
//! pasaporte esté en `01 Identidad` y el PDF en `04 Trámites`. También un
//! suelo; cuenta el nivel **propio** de lo de dentro (su clasificador, su
//! carpeta, lo que dijo el usuario), sin seguir cadenas: si C está en B y B en
//! A, el texto de C está en A y hay arista directa. Y lo que el usuario diga
//! del paquete gana, como con las carpetas.
//!
//! [ADR-0006]: ../../../docs/adr/0006-donde-vive-la-sensibilidad.md
//! [ADR-0020]: ../../../docs/adr/0020-sensibilidad-por-carpeta.md
//! [ADR-0030]: ../../../docs/adr/0030-un-documento-dentro-de-otro.md

use crate::error::Result;
use crate::model::{Assertion, AssertionId, EntityId, Method, Sensitivity, TimePoint};
use crate::store::Store;

/// El predicado de la marca. Cardinalidad `one`: una carpeta tiene una marca o
/// ninguna.
pub const CONTENTS_SENSITIVITY: &str = "contents_sensitivity";

/// Un documento dentro de otro: el de dentro `part_of` el de fuera
/// (ADR-0030). Vive aquí porque de él sale la tercera fuente de sensibilidad;
/// el enlazador que lo escribe usa esta misma constante.
pub const PART_OF: &str = "part_of";

/// Una carpeta que el usuario marcó, con la ruta que cubre.
#[derive(Debug, Clone)]
pub struct FolderMark {
    pub folder: EntityId,
    /// La ruta tal cual la escribió el escáner.
    pub path: String,
    pub level: Sensitivity,
    pub assertion: AssertionId,
    pub asserted_at: i64,
    /// La ruta plegada para comparar. Ver [`fold_path`].
    folded: String,
}

/// Las marcas vigentes, leídas **una vez**.
///
/// Son pocas —las carpetas que el usuario marcó a mano, una docena como mucho—
/// y se leen enteras antes de reproyectar un corpus completo. Leerlas por cada
/// archivo sería un recorrido del log por archivo.
#[derive(Debug, Clone, Default)]
pub struct FolderMarks(Vec<FolderMark>);

impl FolderMarks {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &FolderMark> {
        self.0.iter()
    }

    /// Las marcas que cubren esta ruta.
    pub fn covering<'a>(&'a self, location: &str) -> impl Iterator<Item = &'a FolderMark> + 'a {
        let loc = fold_path(location);
        self.0
            .iter()
            .filter(move |m| within_folded(&loc, &m.folded))
    }

    /// La marca de esta carpeta, si la tiene ella misma (no heredada).
    pub fn of(&self, folder: &EntityId) -> Option<&FolderMark> {
        self.0.iter().find(|m| &m.folder == folder)
    }
}

/// De dónde sale el nivel efectivo. Es la respuesta a *«¿por qué es
/// sensible?»* cuando la respuesta no es una aserción sobre la propia entidad.
#[derive(Debug, Clone)]
pub enum Origin {
    /// Nadie dijo nada: `normal`.
    Default,
    /// La señal más restrictiva del clasificador.
    Classifier,
    /// Una aserción del usuario sobre la propia entidad.
    User,
    /// Una carpeta marcada por el usuario que la contiene.
    Folder(FolderMark),
    /// Un documento que lleva dentro, más sensible que ella (ADR-0030).
    /// `because` es de dónde le viene a **él**: su carpeta, el usuario, su
    /// texto. Es lo que deja decir *«contiene pasaporte.pdf, que es sensible
    /// por tu carpeta 01 Identidad»*.
    Part {
        part: EntityId,
        because: Box<Origin>,
    },
}

#[derive(Debug, Clone)]
pub struct Resolution {
    pub level: Sensitivity,
    pub origin: Origin,
}

/// Minúsculas, para comparar rutas.
///
/// En Windows `C:\Bóveda\01 Identidad` y `c:\bóveda\01 identidad` son la misma
/// carpeta, y renombrar sólo la mayúscula no debería dejar un pasaporte fuera.
/// En Linux serían dos carpetas distintas, y plegar igual sólo puede hacer que
/// una marca cubra **de más**, que es el lado hacia el que se equivoca un
/// control de seguridad.
fn fold_path(p: &str) -> String {
    p.to_lowercase()
}

/// ¿Está `path` dentro de `folder` —o es ella misma—, a cualquier profundidad?
///
/// Con frontera de separador: `C:\Docs` no contiene `C:\Docs viejos\x.pdf`.
pub fn path_within(path: &str, folder: &str) -> bool {
    within_folded(&fold_path(path), &fold_path(folder))
}

fn within_folded(path: &str, folder: &str) -> bool {
    let folder = folder.trim_end_matches(['\\', '/']);
    if folder.is_empty() {
        // La raíz de un sistema tipo Unix: `/` lo contiene todo.
        return path.starts_with(['\\', '/']);
    }
    path == folder || (path.starts_with(folder) && path[folder.len()..].starts_with(['\\', '/']))
}

fn parse(a: &Assertion) -> Option<Sensitivity> {
    a.object.as_text().and_then(Sensitivity::parse)
}

impl Store {
    /// Las carpetas marcadas por el usuario, vigentes en `at`.
    ///
    /// Sólo cuenta el método `user`: una marca que heredan cientos de archivos
    /// es exactamente el tipo de afirmación que un clasificador no puede hacer
    /// por su cuenta ([ADR-0006], alternativa D). Una marca `normal` no es una
    /// marca —es un suelo que no sube nada— y se ignora.
    ///
    /// [ADR-0006]: ../../../docs/adr/0006-donde-vive-la-sensibilidad.md
    pub fn folder_marks(&self, at: TimePoint) -> Result<FolderMarks> {
        let mut out: Vec<FolderMark> = Vec::new();
        // `query` ordena por precedencia y luego por lo más reciente: la
        // primera de cada carpeta es la que manda.
        for a in self.query(None, Some(CONTENTS_SENSITIVITY), at)? {
            if a.method != Method::User || out.iter().any(|m| m.folder == a.subject) {
                continue;
            }
            let Some(level) = parse(&a).filter(|l| *l != Sensitivity::Normal) else {
                continue;
            };
            let Some(path) = self
                .resolve(&a.subject, "folder_path", at)?
                .and_then(|p| p.object.as_text().map(str::to_string))
            else {
                continue;
            };
            out.push(FolderMark {
                folded: fold_path(&path),
                folder: a.subject,
                path,
                level,
                assertion: a.id,
                asserted_at: a.asserted_at,
            });
        }
        Ok(FolderMarks(out))
    }

    /// Dónde está una entidad **ahora**: la ruta de un archivo o la de una
    /// carpeta. `None` para lo que no vive en el disco (una sesión).
    pub fn location(&self, id: &EntityId, at: TimePoint) -> Result<Option<String>> {
        for p in ["path_observed_at", "folder_path"] {
            if let Some(a) = self.resolve(id, p, at)? {
                if let Some(t) = a.object.as_text() {
                    return Ok(Some(t.to_string()));
                }
            }
        }
        Ok(None)
    }

    /// El nivel efectivo de una entidad y de dónde sale, **sin escribirlo**.
    ///
    /// Las reglas, en este orden:
    ///
    /// 1. **La última palabra del usuario gana.** Su aserción sobre la entidad
    ///    manda —incluso para bajar el nivel, como en [ADR-0006]— salvo que una
    ///    carpeta que la contiene se marcara **después** con un nivel mayor. Una
    ///    marca de carpeta nunca baja a nadie.
    /// 2. **Si el usuario no dijo nada de la entidad**, gana lo más restrictivo
    ///    entre el clasificador, las carpetas marcadas que la contienen y los
    ///    documentos que lleva dentro ([ADR-0030]). A igual nivel se explica
    ///    por lo propio: lo de dentro sólo es el porqué cuando sube el nivel.
    ///
    /// [ADR-0006]: ../../../docs/adr/0006-donde-vive-la-sensibilidad.md
    /// [ADR-0030]: ../../../docs/adr/0030-un-documento-dentro-de-otro.md
    pub fn resolve_sensitivity(
        &self,
        id: &EntityId,
        at: TimePoint,
        marks: &FolderMarks,
    ) -> Result<Resolution> {
        let (propia, lo_dijo_el_usuario) = self.resolve_own(id, at, marks)?;
        // Lo que digas del paquete gana: si lo miraste y dijiste «normal», es
        // una excepción consciente, como con las carpetas.
        if lo_dijo_el_usuario {
            return Ok(propia);
        }
        // Lo de dentro, con su nivel **propio**: seguir la cadena no hace
        // falta (el texto de lo de dentro de lo de dentro también está aquí).
        // Entre dos igual de sensibles, el primero por identificador: el
        // porqué no cambia de una vez a otra.
        let mut dentro: Option<(Resolution, EntityId)> = None;
        for a in self.pointing_at(id, Some(PART_OF), at)? {
            let r = self.resolve_own(&a.subject, at, marks)?.0;
            let sube = r.level > propia.level
                && dentro.as_ref().is_none_or(|(m, e)| {
                    r.level > m.level || (r.level == m.level && a.subject.as_str() < e.as_str())
                });
            if sube {
                dentro = Some((r, a.subject));
            }
        }
        Ok(match dentro {
            Some((r, part)) => Resolution {
                level: r.level,
                origin: Origin::Part {
                    part,
                    because: Box::new(r.origin),
                },
            },
            None => propia,
        })
    }

    /// El nivel de una entidad **por sí misma**: lo que se dijo de ella y las
    /// carpetas que la cubren, sin mirar lo que lleva dentro. Y si el usuario
    /// dijo algo de ella.
    fn resolve_own(
        &self,
        id: &EntityId,
        at: TimePoint,
        marks: &FolderMarks,
    ) -> Result<(Resolution, bool)> {
        let claims = self.query(Some(id), Some("sensitivity_level"), at)?;

        // Las carpetas que la cubren, sólo si hay alguna marca: sin marcas no
        // se paga ni la consulta de la ruta.
        let covering: Vec<&FolderMark> = if marks.is_empty() {
            Vec::new()
        } else {
            match self.location(id, at)? {
                Some(loc) => marks.covering(&loc).collect(),
                None => Vec::new(),
            }
        };
        // La más fuerte; a igualdad, la más profunda (es la que el usuario
        // reconocerá como «la carpeta de esto»).
        let strongest = |ms: &mut dyn Iterator<Item = &FolderMark>| -> Option<FolderMark> {
            ms.max_by(|a, b| a.level.cmp(&b.level).then(a.path.len().cmp(&b.path.len())))
                .cloned()
        };

        if let Some(u) = claims.iter().find(|a| a.method == Method::User) {
            let dicho = parse(u).unwrap_or(Sensitivity::Normal);
            // ¿Alguna carpeta marcada después de esto y por encima?
            let mut posteriores = covering
                .iter()
                .copied()
                .filter(|m| m.level > dicho && m.asserted_at > u.asserted_at);
            let r = match strongest(&mut posteriores) {
                Some(m) => Resolution {
                    level: m.level,
                    origin: Origin::Folder(m),
                },
                None => Resolution {
                    level: dicho,
                    origin: Origin::User,
                },
            };
            return Ok((r, true));
        }

        let clasificador = claims.iter().filter_map(parse).max();
        let carpeta = strongest(&mut covering.iter().copied());
        let r = match (clasificador, carpeta) {
            // A igual nivel se cuenta la carpeta: «lo marcaste tú» explica más
            // que una regla del clasificador, y es igual de cierto.
            (c, Some(m)) if c.is_none_or(|c| c <= m.level) => Resolution {
                level: m.level,
                origin: Origin::Folder(m),
            },
            (Some(c), _) => Resolution {
                level: c,
                origin: Origin::Classifier,
            },
            (None, _) => Resolution {
                level: Sensitivity::Normal,
                origin: Origin::Default,
            },
        };
        Ok((r, false))
    }

    /// [`Store::reproject_sensitivity`] con las marcas ya leídas. Es la que
    /// usa quien reproyecta muchas entidades seguidas.
    ///
    /// Reproyecta también **los paquetes que la llevan dentro** (ADR-0030):
    /// marcar la carpeta del pasaporte tapa en el acto el PDF de la visa que
    /// lo contiene, no en la pasada siguiente.
    pub fn reproject_sensitivity_under(
        &self,
        id: &EntityId,
        at: TimePoint,
        marks: &FolderMarks,
    ) -> Result<Sensitivity> {
        let r = self.resolve_sensitivity(id, at, marks)?;
        self.set_sensitivity(id, r.level)?;
        for a in self.query(Some(id), Some(PART_OF), at)? {
            if let Some(paquete) = a.object.as_entity() {
                let nivel = self.resolve_sensitivity(paquete, at, marks)?.level;
                if self.sensitivity(paquete)? != nivel {
                    self.set_sensitivity(paquete, nivel)?;
                }
            }
        }
        Ok(r.level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NewAssertion;

    const T: i64 = 1_000;

    fn carpeta(s: &Store, ruta: &str) -> EntityId {
        let e = s.create_entity(T).unwrap();
        s.assert(NewAssertion::new(
            e.clone(),
            "folder_path",
            ruta,
            Method::Deterministic,
            T,
        ))
        .unwrap();
        e
    }

    fn archivo(s: &Store, ruta: &str) -> EntityId {
        let e = s.create_entity(T).unwrap();
        s.assert(NewAssertion::new(
            e.clone(),
            "path_observed_at",
            ruta,
            Method::Deterministic,
            T,
        ))
        .unwrap();
        e
    }

    fn marcar(s: &Store, c: &EntityId, nivel: &str, at: i64) {
        s.assert(NewAssertion::new(
            c.clone(),
            CONTENTS_SENSITIVITY,
            nivel,
            Method::User,
            at,
        ))
        .unwrap();
    }

    fn dice(s: &Store, e: &EntityId, nivel: &str, metodo: Method, at: i64) {
        s.assert(
            NewAssertion::new(e.clone(), "sensitivity_level", nivel, metodo, at).source("test"),
        )
        .unwrap();
    }

    fn nivel(s: &Store, e: &EntityId, at: i64) -> Sensitivity {
        s.reproject_sensitivity(e, TimePoint::at(at)).unwrap()
    }

    fn resolver(s: &Store, e: &EntityId, at: i64) -> Resolution {
        let tp = TimePoint::at(at);
        s.resolve_sensitivity(e, tp, &s.folder_marks(tp).unwrap())
            .unwrap()
    }

    #[test]
    fn dentro_exige_frontera_de_separador() {
        assert!(path_within(
            r"C:\Bóveda\01 Identidad\tie.jpg",
            r"C:\Bóveda\01 Identidad"
        ));
        assert!(path_within(
            r"C:\Bóveda\01 Identidad\2024\tie.jpg",
            r"C:\Bóveda\01 Identidad"
        ));
        assert!(path_within(
            r"C:\Bóveda\01 Identidad",
            r"C:\Bóveda\01 Identidad"
        ));
        assert!(path_within("/home/jp/Bóveda/x.pdf", "/home/jp/Bóveda/"));
        // Un prefijo que no es carpeta no cuenta.
        assert!(!path_within(r"C:\Docs viejos\x.pdf", r"C:\Docs"));
        assert!(!path_within(
            r"C:\Bóveda\02 Salud\x.pdf",
            r"C:\Bóveda\01 Identidad"
        ));
    }

    #[test]
    fn dentro_no_distingue_mayusculas() {
        assert!(path_within(
            r"c:\bóveda\01 identidad\tie.jpg",
            r"C:\Bóveda\01 Identidad"
        ));
        assert!(path_within(r"C:\BÓVEDA\x.pdf", r"C:\bóveda"));
    }

    #[test]
    fn la_raiz_lo_contiene_todo() {
        assert!(path_within("/x/y.pdf", "/"));
        assert!(path_within(r"C:\x.pdf", r"C:\"));
    }

    /// El caso que abrió la Fase 3: la foto de la TIE no tiene texto ni un
    /// nombre útil, así que ninguna regla la ve. Marcar su carpeta sí.
    #[test]
    fn lo_que_esta_dentro_hereda_a_cualquier_profundidad() {
        let s = Store::open_in_memory().unwrap();
        let identidad = carpeta(&s, r"C:\Bóveda\01 Identidad");
        let tie = archivo(&s, r"C:\Bóveda\01 Identidad\IMG_2291.jpg");
        let hondo = archivo(&s, r"C:\Bóveda\01 Identidad\2024\viejo\dni.png");
        let fuera = archivo(&s, r"C:\Bóveda\02 Salud\receta.pdf");
        assert_eq!(nivel(&s, &tie, T), Sensitivity::Normal);

        marcar(&s, &identidad, "sensitive", T + 1);
        assert_eq!(nivel(&s, &tie, T + 2), Sensitivity::Sensitive);
        assert_eq!(nivel(&s, &hondo, T + 2), Sensitivity::Sensitive);
        assert_eq!(
            nivel(&s, &identidad, T + 2),
            Sensitivity::Sensitive,
            "y la carpeta misma"
        );
        assert_eq!(nivel(&s, &fuera, T + 2), Sensitivity::Normal);
    }

    /// La marca es un suelo: sube y nunca baja lo que ya era bóveda.
    #[test]
    fn la_marca_no_baja_la_boveda() {
        let s = Store::open_in_memory().unwrap();
        let c = carpeta(&s, r"C:\Bóveda\Trabajo");
        let env = archivo(&s, r"C:\Bóveda\Trabajo\.env");
        dice(&s, &env, "vault", Method::Inferred, T);
        marcar(&s, &c, "sensitive", T + 1);
        let r = resolver(&s, &env, T + 2);
        assert_eq!(r.level, Sensitivity::Vault);
        assert!(matches!(r.origin, Origin::Classifier));
    }

    /// A igual nivel, el «¿por qué?» es la carpeta: «lo marcaste tú» explica
    /// más que una regla.
    #[test]
    fn a_igual_nivel_el_porque_es_la_carpeta() {
        let s = Store::open_in_memory().unwrap();
        let c = carpeta(&s, r"C:\Bóveda\01 Identidad");
        let p = archivo(&s, r"C:\Bóveda\01 Identidad\pasaporte.pdf");
        dice(&s, &p, "sensitive", Method::Inferred, T);
        marcar(&s, &c, "sensitive", T + 1);
        match resolver(&s, &p, T + 2).origin {
            Origin::Folder(m) => assert_eq!(m.folder, c),
            otro => panic!("esperaba la carpeta, salió {otro:?}"),
        }
    }

    /// Con dos carpetas marcadas una dentro de otra, gana la más restrictiva.
    #[test]
    fn entre_dos_carpetas_gana_la_mas_restrictiva() {
        let s = Store::open_in_memory().unwrap();
        let fuera = carpeta(&s, r"C:\Bóveda");
        let dentro = carpeta(&s, r"C:\Bóveda\01 Identidad");
        let f = archivo(&s, r"C:\Bóveda\01 Identidad\pasaporte.pdf");
        marcar(&s, &fuera, "vault", T);
        marcar(&s, &dentro, "sensitive", T + 1);
        match resolver(&s, &f, T + 2) {
            Resolution {
                level: Sensitivity::Vault,
                origin: Origin::Folder(m),
            } => {
                assert_eq!(m.folder, fuera)
            }
            otro => panic!("esperaba bóveda por la carpeta de fuera, salió {otro:?}"),
        }
    }

    /// I5 con dos voces del usuario: gana la última. Marcar la carpeta después
    /// de haber dicho «este archivo es normal» lo sube; decir «normal» después
    /// de marcar la carpeta es una excepción consciente y se respeta.
    #[test]
    fn gana_la_ultima_palabra_del_usuario() {
        let s = Store::open_in_memory().unwrap();
        let c = carpeta(&s, r"C:\Bóveda\04 Trámites");
        let antes = archivo(&s, r"C:\Bóveda\04 Trámites\gimnasio.pdf");
        let despues = archivo(&s, r"C:\Bóveda\04 Trámites\folleto.pdf");

        dice(&s, &antes, "normal", Method::User, T);
        marcar(&s, &c, "sensitive", T + 10);
        dice(&s, &despues, "normal", Method::User, T + 20);

        assert_eq!(nivel(&s, &antes, T + 30), Sensitivity::Sensitive);
        assert_eq!(nivel(&s, &despues, T + 30), Sensitivity::Normal);
        assert!(matches!(
            resolver(&s, &despues, T + 30).origin,
            Origin::User
        ));
    }

    /// Subir a mano por encima de la carpeta siempre vale, sea antes o después.
    #[test]
    fn el_usuario_puede_subir_por_encima_de_la_carpeta() {
        let s = Store::open_in_memory().unwrap();
        let c = carpeta(&s, r"C:\Bóveda\01 Identidad");
        let p = archivo(&s, r"C:\Bóveda\01 Identidad\claves.txt");
        dice(&s, &p, "vault", Method::User, T);
        marcar(&s, &c, "sensitive", T + 1);
        assert_eq!(nivel(&s, &p, T + 2), Sensitivity::Vault);
    }

    /// La herencia sigue a donde está el archivo **ahora** (I8: la ruta es una
    /// observación). Lo que entra en la carpeta hereda; lo que sale, deja de
    /// heredar.
    #[test]
    fn la_herencia_sigue_a_la_ruta_de_ahora() {
        let s = Store::open_in_memory().unwrap();
        let c = carpeta(&s, r"C:\Bóveda\01 Identidad");
        let f = archivo(&s, r"C:\Bóveda\Descargas\scan.jpg");
        marcar(&s, &c, "sensitive", T);
        assert_eq!(nivel(&s, &f, T + 1), Sensitivity::Normal);

        let entra = r"C:\Bóveda\01 Identidad\scan.jpg";
        s.assert(NewAssertion::new(
            f.clone(),
            "path_observed_at",
            entra,
            Method::Deterministic,
            T + 2,
        ))
        .unwrap();
        assert_eq!(
            nivel(&s, &f, T + 3),
            Sensitivity::Sensitive,
            "entra y hereda"
        );

        let sale = r"C:\Bóveda\Descargas\scan.jpg";
        s.assert(NewAssertion::new(
            f.clone(),
            "path_observed_at",
            sale,
            Method::Deterministic,
            T + 4,
        ))
        .unwrap();
        assert_eq!(
            nivel(&s, &f, T + 5),
            Sensitivity::Normal,
            "sale y deja de heredar"
        );
    }

    /// Sólo el usuario marca carpetas. Una marca inferida —que nadie escribe
    /// hoy, pero alguien podría mañana— no hereda nada.
    #[test]
    fn una_marca_que_no_es_del_usuario_no_cuenta() {
        let s = Store::open_in_memory().unwrap();
        let c = carpeta(&s, r"C:\Bóveda\Contratos");
        let f = archivo(&s, r"C:\Bóveda\Contratos\alquiler.pdf");
        s.assert(NewAssertion::new(
            c.clone(),
            CONTENTS_SENSITIVITY,
            "vault",
            Method::Inferred,
            T,
        ))
        .unwrap();
        assert_eq!(nivel(&s, &f, T + 1), Sensitivity::Normal);
        assert!(s.folder_marks(TimePoint::at(T + 1)).unwrap().is_empty());
    }

    /// Retractar la marca la quita, y el pasado sigue siendo consultable (I6):
    /// *¿qué protegía yo antes de quitarla?*
    #[test]
    fn quitar_la_marca_no_borra_que_existio() {
        let s = Store::open_in_memory().unwrap();
        let c = carpeta(&s, r"C:\Bóveda\01 Identidad");
        marcar(&s, &c, "sensitive", T);
        let id = s
            .folder_marks(TimePoint::at(T + 1))
            .unwrap()
            .of(&c)
            .unwrap()
            .assertion
            .clone();
        s.retract(&id, T + 5).unwrap();

        assert!(s.folder_marks(TimePoint::at(T + 6)).unwrap().is_empty());
        assert!(
            s.folder_marks(TimePoint::at(T + 1))
                .unwrap()
                .of(&c)
                .is_some()
        );
    }

    // ── lo que lleva dentro (ADR-0030) ────────────────────────────────────

    fn dentro_de(s: &Store, parte: &EntityId, paquete: &EntityId) {
        s.assert(NewAssertion::new(
            parte.clone(),
            PART_OF,
            crate::model::Value::Entity(paquete.clone()),
            Method::Deterministic,
            T,
        ))
        .unwrap();
    }

    /// El caso que lo pidió: el pasaporte está en una carpeta marcada y el PDF
    /// de la visa, fuera, lo lleva dentro. Sin esto saldría sin tapar.
    #[test]
    fn un_paquete_es_tan_sensible_como_lo_que_lleva_dentro() {
        let s = Store::open_in_memory().unwrap();
        let identidad = carpeta(&s, r"C:\Bóveda\01 Identidad");
        let pasaporte = archivo(&s, r"C:\Bóveda\01 Identidad\pasaporte.pdf");
        let visa = archivo(&s, r"C:\Bóveda\04 Trámites\visa.pdf");
        dentro_de(&s, &pasaporte, &visa);
        marcar(&s, &identidad, "sensitive", T + 1);

        let r = resolver(&s, &visa, T + 2);
        assert_eq!(r.level, Sensitivity::Sensitive);
        match r.origin {
            Origin::Part { part, because } => {
                assert_eq!(part, pasaporte);
                assert!(matches!(*because, Origin::Folder(ref m) if m.folder == identidad));
            }
            otro => panic!("esperaba lo de dentro, salió {otro:?}"),
        }
        // Y al revés no: lo de dentro no se contagia de su paquete.
        let otro = archivo(&s, r"C:\Bóveda\04 Trámites\carta.pdf");
        dice(&s, &visa, "vault", Method::Inferred, T + 3);
        dentro_de(&s, &otro, &visa);
        assert_eq!(resolver(&s, &otro, T + 4).level, Sensitivity::Normal);
    }

    /// Si miraste el paquete y dijiste «normal», es tu excepción.
    #[test]
    fn lo_que_digas_del_paquete_gana_a_lo_de_dentro() {
        let s = Store::open_in_memory().unwrap();
        let pasaporte = archivo(&s, r"C:\Bóveda\pasaporte.pdf");
        let visa = archivo(&s, r"C:\Bóveda\visa.pdf");
        dentro_de(&s, &pasaporte, &visa);
        dice(&s, &pasaporte, "sensitive", Method::User, T + 1);
        dice(&s, &visa, "normal", Method::User, T + 2);
        let r = resolver(&s, &visa, T + 3);
        assert_eq!(r.level, Sensitivity::Normal);
        assert!(matches!(r.origin, Origin::User));
    }

    /// A igual nivel se explica por lo propio; lo de dentro sólo cuando sube.
    #[test]
    fn lo_de_dentro_solo_es_el_porque_cuando_sube_el_nivel() {
        let s = Store::open_in_memory().unwrap();
        let pasaporte = archivo(&s, r"C:\Bóveda\pasaporte.pdf");
        let env = archivo(&s, r"C:\Bóveda\.env");
        let visa = archivo(&s, r"C:\Bóveda\visa.pdf");
        dentro_de(&s, &pasaporte, &visa);
        dice(&s, &pasaporte, "sensitive", Method::Inferred, T);
        dice(&s, &visa, "sensitive", Method::Inferred, T);
        assert!(matches!(
            resolver(&s, &visa, T + 1).origin,
            Origin::Classifier
        ));

        // Algo de bóveda dentro sí sube.
        dice(&s, &env, "vault", Method::Inferred, T + 2);
        dentro_de(&s, &env, &visa);
        let r = resolver(&s, &visa, T + 3);
        assert_eq!(r.level, Sensitivity::Vault);
        assert!(matches!(r.origin, Origin::Part { ref part, .. } if *part == env));
    }

    /// Marcar la carpeta de lo de dentro tapa el paquete en el acto, y
    /// quitarle la marca lo destapa.
    #[test]
    fn reproyectar_lo_de_dentro_reproyecta_su_paquete() {
        let s = Store::open_in_memory().unwrap();
        let identidad = carpeta(&s, r"C:\Bóveda\01 Identidad");
        let pasaporte = archivo(&s, r"C:\Bóveda\01 Identidad\pasaporte.pdf");
        let visa = archivo(&s, r"C:\Bóveda\04 Trámites\visa.pdf");
        dentro_de(&s, &pasaporte, &visa);
        assert_eq!(nivel(&s, &visa, T), Sensitivity::Normal);

        marcar(&s, &identidad, "sensitive", T + 1);
        nivel(&s, &pasaporte, T + 2);
        assert_eq!(s.sensitivity(&visa).unwrap(), Sensitivity::Sensitive);

        let marca = s
            .folder_marks(TimePoint::at(T + 2))
            .unwrap()
            .of(&identidad)
            .unwrap()
            .assertion
            .clone();
        s.retract(&marca, T + 3).unwrap();
        nivel(&s, &pasaporte, T + 4);
        assert_eq!(s.sensitivity(&visa).unwrap(), Sensitivity::Normal);
    }
}
