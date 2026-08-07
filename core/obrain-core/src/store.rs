//! El log de aserciones y sus proyecciones.
//!
//! Toda escritura es un `INSERT`. La única mutación permitida en todo el
//! sistema es fijar `retracted_at`, y los triggers de `schema.rs` garantizan
//! que no hay otra.

use crate::error::{Error, Result};
use crate::model::*;
use crate::schema::{DDL, SCHEMA_VERSION, SEED_PREDICATES};
use rusqlite::{Connection, OptionalExtension, named_params};

const COLS: &str = "id, subject, predicate, object_kind, object_entity, object_text, \
                    object_number, valid_from, valid_to, asserted_at, retracted_at, \
                    method, source, confidence, evidence";

/// Predicado bitemporal, en SQL. Es el corazón del invariante I6 y aparece
/// en toda consulta: qué era cierto en `:valid`, según lo que el sistema
/// sabía en `:known`.
const BITEMPORAL: &str = "
        asserted_at <= :known
    AND (retracted_at IS NULL OR retracted_at > :known)
    AND (valid_from   IS NULL OR valid_from  <= :valid)
    AND (valid_to     IS NULL OR valid_to    >  :valid)";

/// Orden de precedencia al resolver un conflicto. `user` gana siempre (I5).
const PRECEDENCE: &str = "
    ORDER BY CASE method WHEN 'user' THEN 3 WHEN 'deterministic' THEN 2
                         WHEN 'imported' THEN 1 ELSE 0 END DESC,
             asserted_at DESC,
             confidence  DESC";

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open_in_memory() -> Result<Self> {
        let store = Store { conn: Connection::open_in_memory()? };
        store.migrate()?;
        Ok(store)
    }

    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let store = Store { conn: Connection::open(path)? };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(DDL)?;
        self.conn
            .pragma_update(None, "user_version", SCHEMA_VERSION)?;
        for (id, label, card, method, inverse) in SEED_PREDICATES {
            self.conn.execute(
                "INSERT OR IGNORE INTO predicates
                     (id, label, cardinality, symmetric, transitive, inverse, default_method)
                 VALUES (?1, ?2, ?3, 0, 0, ?4, ?5)",
                rusqlite::params![id, label, card, inverse, method],
            )?;
        }
        Ok(())
    }

    /// Acceso de sólo lectura a la conexión, para consultas ad hoc en tests.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    // ───────────────────────────────────────────────────────────── entidades

    pub fn create_entity(&self, created_at: i64) -> Result<EntityId> {
        self.create_entity_with(created_at, Sensitivity::Normal)
    }

    pub fn create_entity_with(&self, created_at: i64, s: Sensitivity) -> Result<EntityId> {
        let id = EntityId::new();
        self.conn.execute(
            "INSERT INTO entities (id, created_at, sensitivity) VALUES (?1, ?2, ?3)",
            rusqlite::params![id.as_str(), created_at, s.as_str()],
        )?;
        Ok(id)
    }

    pub fn sensitivity(&self, id: &EntityId) -> Result<Sensitivity> {
        let s: String = self
            .conn
            .query_row(
                "SELECT sensitivity FROM entities WHERE id = ?1",
                [id.as_str()],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| Error::UnknownEntity(id.to_string()))?;
        Sensitivity::parse(&s).ok_or_else(|| Error::Corrupt(format!("sensibilidad '{s}'")))
    }

    /// Cambiar sensibilidad es una operación de seguridad, no una aserción.
    pub fn set_sensitivity(&self, id: &EntityId, s: Sensitivity) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE entities SET sensitivity = ?2 WHERE id = ?1",
            rusqlite::params![id.as_str(), s.as_str()],
        )?;
        if n == 0 {
            return Err(Error::UnknownEntity(id.to_string()));
        }
        Ok(())
    }

    // ──────────────────────────────────────────────────────────── predicados

    pub fn register_predicate(&self, def: &PredicateDef) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO predicates
                 (id, label, cardinality, symmetric, transitive, inverse, default_method)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                def.id,
                def.label,
                def.cardinality.as_str(),
                def.symmetric as i32,
                def.transitive as i32,
                def.inverse,
                def.default_method.as_str(),
            ],
        )?;
        Ok(())
    }

    pub fn cardinality(&self, predicate: &str) -> Result<Cardinality> {
        let c: String = self
            .conn
            .query_row(
                "SELECT cardinality FROM predicates WHERE id = ?1",
                [predicate],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| Error::UnknownPredicate(predicate.to_string()))?;
        Cardinality::parse(&c).ok_or_else(|| Error::Corrupt(format!("cardinalidad '{c}'")))
    }

    // ──────────────────────────────────────────────────────────── aserciones

    /// Escribe una aserción. Nunca sobrescribe: siempre añade al log.
    pub fn assert(&self, a: NewAssertion) -> Result<AssertionId> {
        let id = AssertionId::new();
        let (kind, ent, txt, num) = decompose(&a.object);
        self.conn.execute(
            &format!("INSERT INTO assertions ({COLS}) VALUES \
                      (:id, :subject, :predicate, :kind, :ent, :txt, :num, \
                       :valid_from, :valid_to, :asserted_at, NULL, \
                       :method, :source, :confidence, :evidence)"),
            named_params! {
                ":id":          id.as_str(),
                ":subject":     a.subject.as_str(),
                ":predicate":   a.predicate,
                ":kind":        kind,
                ":ent":         ent,
                ":txt":         txt,
                ":num":         num,
                ":valid_from":  a.valid_from,
                ":valid_to":    a.valid_to,
                ":asserted_at": a.asserted_at,
                ":method":      a.method.as_str(),
                ":source":      a.source,
                ":confidence":  a.confidence,
                ":evidence":    a.evidence,
            },
        )?;
        Ok(id)
    }

    /// Retracta una aserción. **No la borra** (I4): la marca como no vigente
    /// a partir de `at`. Consultarla con un `known` anterior sigue devolviéndola,
    /// que es lo que hace posible "¿qué creía yo en marzo de 2023?".
    pub fn retract(&self, id: &AssertionId, at: i64) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE assertions SET retracted_at = ?2 WHERE id = ?1",
            rusqlite::params![id.as_str(), at],
        )?;
        if n == 0 {
            return Err(Error::UnknownAssertion(id.to_string()));
        }
        Ok(())
    }

    /// Todas las aserciones vigentes que cumplen el filtro, en el punto
    /// bitemporal dado.
    pub fn query(
        &self,
        subject: Option<&EntityId>,
        predicate: Option<&str>,
        at: TimePoint,
    ) -> Result<Vec<Assertion>> {
        let sql = format!(
            "SELECT {COLS} FROM assertions
             WHERE (:subject   IS NULL OR subject   = :subject)
               AND (:predicate IS NULL OR predicate = :predicate)
               AND {BITEMPORAL}
             {PRECEDENCE}"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            named_params! {
                ":subject":   subject.map(|s| s.as_str()),
                ":predicate": predicate,
                ":valid":     at.valid,
                ":known":     at.known,
            },
            row_to_assertion,
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Resuelve el valor **único** de `(sujeto, predicado)` aplicando precedencia
    /// de método. Es la proyección que implementa el invariante I5: si el usuario
    /// ha afirmado algo, ninguna inferencia posterior puede desplazarlo.
    pub fn resolve(
        &self,
        subject: &EntityId,
        predicate: &str,
        at: TimePoint,
    ) -> Result<Option<Assertion>> {
        let sql = format!(
            "SELECT {COLS} FROM assertions
             WHERE subject = :subject AND predicate = :predicate AND {BITEMPORAL}
             {PRECEDENCE} LIMIT 1"
        );
        Ok(self
            .conn
            .query_row(
                &sql,
                named_params! {
                    ":subject":   subject.as_str(),
                    ":predicate": predicate,
                    ":valid":     at.valid,
                    ":known":     at.known,
                },
                row_to_assertion,
            )
            .optional()?)
    }

    /// Facetas vigentes de una entidad. El "tipo" de una entidad es esto.
    pub fn facets(&self, subject: &EntityId, at: TimePoint) -> Result<Vec<String>> {
        Ok(self
            .query(Some(subject), Some("has_facet"), at)?
            .into_iter()
            .filter_map(|a| a.object.as_text().map(str::to_string))
            .collect())
    }

    /// Contradicciones: predicados de cardinalidad `one` con más de un valor
    /// distinto vigente a la vez.
    ///
    /// No es un job de detección aparte — es una consulta. Ésa es la ventaja
    /// estructural de que la unidad primaria sea la aserción y no el nodo.
    pub fn contradictions(&self, at: TimePoint) -> Result<Vec<Contradiction>> {
        let sql = format!(
            "SELECT a.subject, a.predicate
             FROM assertions a JOIN predicates p ON p.id = a.predicate
             WHERE p.cardinality = 'one' AND {BITEMPORAL}
             GROUP BY a.subject, a.predicate
             HAVING COUNT(DISTINCT COALESCE(a.object_entity, a.object_text,
                                            CAST(a.object_number AS TEXT))) > 1"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let keys: Vec<(String, String)> = stmt
            .query_map(
                named_params! { ":valid": at.valid, ":known": at.known },
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut out = Vec::new();
        for (subject, predicate) in keys {
            let subject = EntityId(subject);
            let assertions = self.query(Some(&subject), Some(&predicate), at)?;
            out.push(Contradiction { subject, predicate, assertions });
        }
        Ok(out)
    }

    /// Número total de aserciones escritas, incluidas las retractadas.
    /// El log sólo crece.
    pub fn log_len(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM assertions", [], |r| r.get(0))?)
    }
}

/// Dos o más aserciones vigentes e incompatibles sobre lo mismo.
#[derive(Debug)]
pub struct Contradiction {
    pub subject: EntityId,
    pub predicate: String,
    /// Ordenadas por precedencia: la primera es la que gana.
    pub assertions: Vec<Assertion>,
}

// ─────────────────────────────────────────────────────────────────── helpers

type Decomposed = (&'static str, Option<String>, Option<String>, Option<f64>);

fn decompose(v: &Value) -> Decomposed {
    match v {
        Value::Entity(e) => ("entity", Some(e.0.clone()), None, None),
        Value::Text(t) => ("text", None, Some(t.clone()), None),
        Value::Number(n) => ("number", None, None, Some(*n)),
        // i64 de epoch en ms cabe exacto en el mantisa de f64 (< 2^53)
        Value::Timestamp(t) => ("timestamp", None, None, Some(*t as f64)),
        Value::Bool(b) => ("bool", None, None, Some(if *b { 1.0 } else { 0.0 })),
    }
}

fn row_to_assertion(row: &rusqlite::Row<'_>) -> rusqlite::Result<Assertion> {
    let kind: String = row.get(3)?;
    let object = match kind.as_str() {
        "entity" => Value::Entity(EntityId(row.get(4)?)),
        "text" => Value::Text(row.get(5)?),
        "number" => Value::Number(row.get(6)?),
        "timestamp" => Value::Timestamp(row.get::<_, f64>(6)? as i64),
        "bool" => Value::Bool(row.get::<_, f64>(6)? != 0.0),
        other => {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(Error::Corrupt(format!("object_kind '{other}'"))),
            ));
        }
    };
    let method_str: String = row.get(11)?;
    let method = Method::parse(&method_str).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            11,
            rusqlite::types::Type::Text,
            Box::new(Error::Corrupt(format!("method '{method_str}'"))),
        )
    })?;

    Ok(Assertion {
        id: AssertionId(row.get(0)?),
        subject: EntityId(row.get(1)?),
        predicate: row.get(2)?,
        object,
        valid_from: row.get(7)?,
        valid_to: row.get(8)?,
        asserted_at: row.get(9)?,
        retracted_at: row.get(10)?,
        method,
        source: row.get(12)?,
        confidence: row.get(13)?,
        evidence: row.get(14)?,
    })
}
