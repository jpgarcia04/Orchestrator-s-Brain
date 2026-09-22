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
//! [ADR-0006]: ../../../docs/adr/0006-donde-vive-la-sensibilidad.md
//! [ADR-0020]: ../../../docs/adr/0020-sensibilidad-por-carpeta.md

use crate::error::Result;
use crate::model::{Assertion, AssertionId, EntityId, Method, Sensitivity, TimePoint};
use crate::store::Store;

/// El predicado de la marca. Cardinalidad `one`: una carpeta tiene una marca o
/// ninguna.
pub const CONTENTS_SENSITIVITY: &str = "contents_sensitivity";

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
        self.0.iter().filter(move |m| within_folded(&loc, &m.folded))
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
    path == folder
        || (path.starts_with(folder) && path[folder.len()..].starts_with(['\\', '/']))
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
    ///    entre el clasificador y las carpetas marcadas que la contienen.
    ///
    /// [ADR-0006]: ../../../docs/adr/0006-donde-vive-la-sensibilidad.md
    pub fn resolve_sensitivity(
        &self,
        id: &EntityId,
        at: TimePoint,
        marks: &FolderMarks,
    ) -> Result<Resolution> {
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
            return Ok(match strongest(&mut posteriores) {
                Some(m) => Resolution { level: m.level, origin: Origin::Folder(m) },
                None => Resolution { level: dicho, origin: Origin::User },
            });
        }

        let clasificador = claims.iter().filter_map(parse).max();
        let carpeta = strongest(&mut covering.iter().copied());
        Ok(match (clasificador, carpeta) {
            // A igual nivel se cuenta la carpeta: «lo marcaste tú» explica más
            // que una regla del clasificador, y es igual de cierto.
            (c, Some(m)) if c.is_none_or(|c| c <= m.level) => {
                Resolution { level: m.level, origin: Origin::Folder(m) }
            }
            (Some(c), _) => Resolution { level: c, origin: Origin::Classifier },
            (None, _) => Resolution { level: Sensitivity::Normal, origin: Origin::Default },
        })
    }

    /// [`Store::reproject_sensitivity`] con las marcas ya leídas. Es la que
    /// usa quien reproyecta muchas entidades seguidas.
    pub fn reproject_sensitivity_under(
        &self,
        id: &EntityId,
        at: TimePoint,
        marks: &FolderMarks,
    ) -> Result<Sensitivity> {
        let r = self.resolve_sensitivity(id, at, marks)?;
        self.set_sensitivity(id, r.level)?;
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
        s.assert(NewAssertion::new(e.clone(), "folder_path", ruta, Method::Deterministic, T))
            .unwrap();
        e
    }

    fn archivo(s: &Store, ruta: &str) -> EntityId {
        let e = s.create_entity(T).unwrap();
        s.assert(NewAssertion::new(e.clone(), "path_observed_at", ruta, Method::Deterministic, T))
            .unwrap();
        e
    }

    fn marcar(s: &Store, c: &EntityId, nivel: &str, at: i64) {
        s.assert(NewAssertion::new(c.clone(), CONTENTS_SENSITIVITY, nivel, Method::User, at))
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
        s.resolve_sensitivity(e, tp, &s.folder_marks(tp).unwrap()).unwrap()
    }

    #[test]
    fn dentro_exige_frontera_de_separador() {
        assert!(path_within(r"C:\Bóveda\01 Identidad\tie.jpg", r"C:\Bóveda\01 Identidad"));
        assert!(path_within(r"C:\Bóveda\01 Identidad\2024\tie.jpg", r"C:\Bóveda\01 Identidad"));
        assert!(path_within(r"C:\Bóveda\01 Identidad", r"C:\Bóveda\01 Identidad"));
        assert!(path_within("/home/jp/Bóveda/x.pdf", "/home/jp/Bóveda/"));
        // Un prefijo que no es carpeta no cuenta.
        assert!(!path_within(r"C:\Docs viejos\x.pdf", r"C:\Docs"));
        assert!(!path_within(r"C:\Bóveda\02 Salud\x.pdf", r"C:\Bóveda\01 Identidad"));
    }

    #[test]
    fn dentro_no_distingue_mayusculas() {
        assert!(path_within(r"c:\bóveda\01 identidad\tie.jpg", r"C:\Bóveda\01 Identidad"));
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
        assert_eq!(nivel(&s, &identidad, T + 2), Sensitivity::Sensitive, "y la carpeta misma");
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
            Resolution { level: Sensitivity::Vault, origin: Origin::Folder(m) } => {
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
        assert!(matches!(resolver(&s, &despues, T + 30).origin, Origin::User));
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
        s.assert(NewAssertion::new(f.clone(), "path_observed_at", entra, Method::Deterministic, T + 2))
            .unwrap();
        assert_eq!(nivel(&s, &f, T + 3), Sensitivity::Sensitive, "entra y hereda");

        let sale = r"C:\Bóveda\Descargas\scan.jpg";
        s.assert(NewAssertion::new(f.clone(), "path_observed_at", sale, Method::Deterministic, T + 4))
            .unwrap();
        assert_eq!(nivel(&s, &f, T + 5), Sensitivity::Normal, "sale y deja de heredar");
    }

    /// Sólo el usuario marca carpetas. Una marca inferida —que nadie escribe
    /// hoy, pero alguien podría mañana— no hereda nada.
    #[test]
    fn una_marca_que_no_es_del_usuario_no_cuenta() {
        let s = Store::open_in_memory().unwrap();
        let c = carpeta(&s, r"C:\Bóveda\Contratos");
        let f = archivo(&s, r"C:\Bóveda\Contratos\alquiler.pdf");
        s.assert(NewAssertion::new(c.clone(), CONTENTS_SENSITIVITY, "vault", Method::Inferred, T))
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
        let id = s.folder_marks(TimePoint::at(T + 1)).unwrap().of(&c).unwrap().assertion.clone();
        s.retract(&id, T + 5).unwrap();

        assert!(s.folder_marks(TimePoint::at(T + 6)).unwrap().is_empty());
        assert!(s.folder_marks(TimePoint::at(T + 1)).unwrap().of(&c).is_some());
    }
}
