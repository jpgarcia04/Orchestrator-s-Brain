//! **La puerta.** Toda capacidad de IA pasa por aquí antes de tocar contenido.
//!
//! El invariante I11 dice que el egreso es explícito y que la política se
//! *aplica*, no se configura. Esto es esa aplicación: la tabla de
//! `PROJECT-MASTER.md` §5.2 escrita como código que se puede probar.
//!
//! | Nivel | Índice vectorial | Modelo remoto |
//! |---|---|---|
//! | `normal` | sí | sí, con consentimiento |
//! | `sensitive` | sí, local | **nunca** |
//! | `vault` | **nunca** | **nunca** |
//!
//! La búsqueda léxica queda deliberadamente **fuera** de la puerta. No es una
//! omisión: §5.2 concede índice léxico local incluso a `vault`, porque BM25 es
//! determinista, corre en el proceso y no produce un artefacto del que se
//! pueda reconstruir el original. Un vector sí lo es, y por eso sí pasa.
//!
//! ## Falla cerrado
//!
//! Una entidad que no existe, o cuya sensibilidad no se puede leer, se trata
//! como `vault`. Un control de seguridad que ante un error deja pasar no es un
//! control: es una comprobación decorativa.

use obrain_core::rusqlite::Connection;
use obrain_core::{EntityId, Result, Sensitivity, Store};

/// Las capacidades de `PROJECT-MASTER.md` §5.1. La puerta no distingue entre
/// ellas para negar —lo que decide es el nivel y el destino—, pero el motivo
/// del rechazo tiene que poder decir qué se intentaba.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Embedder,
    Extractor,
    Classifier,
    Summarizer,
    Answerer,
    Reranker,
}

impl Capability {
    pub fn as_str(self) -> &'static str {
        match self {
            Capability::Embedder => "embedder",
            Capability::Extractor => "extractor",
            Capability::Classifier => "classifier",
            Capability::Summarizer => "summarizer",
            Capability::Answerer => "answerer",
            Capability::Reranker => "reranker",
        }
    }

    /// ¿Produce esta capacidad un artefacto persistente derivado del contenido?
    ///
    /// Un embedding lo es: se guarda, sobrevive al archivo y de él se puede
    /// recuperar bastante del original. Un rerank no: es una puntuación que se
    /// tira. Por eso `vault` puede pasar por un reranker local y no por un
    /// embedder local.
    pub fn persists_derivative(self) -> bool {
        matches!(self, Capability::Embedder)
    }
}

/// Dónde va a ejecutarse el modelo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runs {
    /// En esta máquina. El contenido no cruza el proceso.
    Local,
    /// En un proveedor remoto. El contenido sale del equipo.
    Remote,
}

/// Resultado de la comprobación. `#[must_use]` a propósito: ignorar la
/// respuesta de la puerta tiene que ser un error de compilación, no un
/// descuido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum Decision {
    Allow,
    /// Permitido, pero el usuario tiene que haberlo consentido explícitamente
    /// para este destino. §5.2: `normal` sale «con consentimiento».
    NeedsConsent,
    Deny(&'static str),
}

impl Decision {
    pub fn allowed(self) -> bool {
        matches!(self, Decision::Allow)
    }

    pub fn reason(self) -> Option<&'static str> {
        match self {
            Decision::Deny(r) => Some(r),
            _ => None,
        }
    }
}

/// La decisión, dado un nivel ya leído. Función pura: es la que se prueba.
pub fn decide(level: Sensitivity, capability: Capability, runs: Runs) -> Decision {
    match (level, runs) {
        (Sensitivity::Vault, _) => {
            if capability.persists_derivative() || runs == Runs::Remote {
                Decision::Deny("vault: ni sale de la maquina ni deja derivados persistentes")
            } else {
                Decision::Allow
            }
        }
        (Sensitivity::Sensitive, Runs::Remote) => {
            Decision::Deny("sensitive: solo modelos locales")
        }
        (Sensitivity::Sensitive, Runs::Local) => Decision::Allow,
        (Sensitivity::Normal, Runs::Local) => Decision::Allow,
        (Sensitivity::Normal, Runs::Remote) => Decision::NeedsConsent,
    }
}

/// La decisión para una entidad concreta, leyendo la columna.
///
/// Es un `SELECT` de una columna indexada, no una resolución del log. Ése es el
/// motivo entero de que la sensibilidad viva en `entities` (ADR-0006).
pub fn check(
    store: &Store,
    entity: &EntityId,
    capability: Capability,
    runs: Runs,
) -> Result<Decision> {
    // Falla cerrado: si no sabemos qué es, es bóveda.
    let level = store.sensitivity(entity).unwrap_or(Sensitivity::Vault);
    Ok(decide(level, capability, runs))
}

/// Las entidades que **pueden** entrar en el índice vectorial.
///
/// Es la consulta que M4 tiene que usar como origen en vez de `SELECT DISTINCT
/// entity FROM chunks`. La diferencia entre ambas es exactamente la diferencia
/// entre cumplir I11 y decir que se cumple.
pub fn embeddable(conn: &Connection) -> Result<Vec<EntityId>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT c.entity
           FROM chunks c JOIN entities e ON e.id = c.entity
          WHERE e.sensitivity <> 'vault'
          ORDER BY c.entity",
    )?;
    let rows = stmt.query_map([], |r| Ok(EntityId(r.get(0)?)))?;
    Ok(rows.collect::<obrain_core::rusqlite::Result<Vec<_>>>()?)
}

/// Cuántos fragmentos quedan fuera del índice vectorial por ser de bóveda.
/// Un número que el informe tiene que enseñar: es el coste real de la política.
pub fn withheld_chunks(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM chunks c JOIN entities e ON e.id = c.entity
          WHERE e.sensitivity = 'vault'",
        [],
        |r| r.get(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use obrain_core::{Method, NewAssertion, TimePoint};

    #[test]
    fn la_boveda_nunca_se_vectoriza_ni_local() {
        let d = decide(Sensitivity::Vault, Capability::Embedder, Runs::Local);
        assert!(!d.allowed());
        assert!(d.reason().is_some());
    }

    #[test]
    fn la_boveda_nunca_sale() {
        for cap in [Capability::Summarizer, Capability::Answerer, Capability::Reranker] {
            assert!(!decide(Sensitivity::Vault, cap, Runs::Remote).allowed());
        }
    }

    /// Un rerank no persiste nada, así que la bóveda sí puede pasar por él en
    /// local. Es la única puerta abierta que tiene `vault`, y es intencionada.
    #[test]
    fn la_boveda_admite_capacidades_locales_sin_derivado() {
        assert!(decide(Sensitivity::Vault, Capability::Reranker, Runs::Local).allowed());
    }

    #[test]
    fn lo_sensible_se_queda_en_casa() {
        assert!(decide(Sensitivity::Sensitive, Capability::Embedder, Runs::Local).allowed());
        assert!(!decide(Sensitivity::Sensitive, Capability::Embedder, Runs::Remote).allowed());
        assert!(!decide(Sensitivity::Sensitive, Capability::Answerer, Runs::Remote).allowed());
    }

    #[test]
    fn lo_normal_sale_solo_con_consentimiento() {
        assert_eq!(
            decide(Sensitivity::Normal, Capability::Answerer, Runs::Remote),
            Decision::NeedsConsent
        );
        assert!(decide(Sensitivity::Normal, Capability::Embedder, Runs::Local).allowed());
    }

    /// Una entidad desconocida se trata como bóveda. Ante un fallo, no se filtra.
    #[test]
    fn una_entidad_desconocida_falla_cerrado() {
        let store = Store::open_in_memory().unwrap();
        let fantasma = EntityId::from("NO-EXISTE");
        let d = check(&store, &fantasma, Capability::Embedder, Runs::Local).unwrap();
        assert!(!d.allowed());
    }

    /// La consulta que M4 debe usar como origen del índice vectorial excluye
    /// la bóveda. Es el test que convierte la política en un hecho.
    #[test]
    fn embeddable_excluye_la_boveda() {
        let store = Store::open_in_memory().unwrap();
        obrain_extract::index::migrate(store.conn()).unwrap();
        let at = 1_000;
        let tp = TimePoint::at(at);

        let normal = store.create_entity(at).unwrap();
        let secreto = store.create_entity(at).unwrap();
        for e in [&normal, &secreto] {
            store
                .conn()
                .execute(
                    "INSERT INTO chunks (entity, chunker, chunker_ver, ordinal,
                                         byte_start, byte_end, text)
                     VALUES (?1, 'test', 1, 0, 0, 5, 'hola')",
                    [e.as_str()],
                )
                .unwrap();
        }
        store
            .assert(
                NewAssertion::new(secreto.clone(), "sensitivity_level", "vault", Method::Inferred, at)
                    .source("guard:test"),
            )
            .unwrap();
        store.reproject_sensitivity(&secreto, tp).unwrap();

        let ok = embeddable(store.conn()).unwrap();
        assert_eq!(ok, vec![normal.clone()]);
        assert!(!ok.contains(&secreto));
        assert_eq!(withheld_chunks(store.conn()).unwrap(), 1);
    }
}
