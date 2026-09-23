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
        let store = Store {
            conn: Connection::open_in_memory()?,
        };
        store.migrate()?;
        Ok(store)
    }

    /// Abre una base del disco.
    ///
    /// **Si tiene llave, está cifrada** (ADR-0021) y se abre con la llave que
    /// guarda el sistema —DPAPI en Windows—, sin preguntar nada. Si no la
    /// tiene, se abre en claro, como siempre. Crear la llave de una base nueva
    /// o cifrar una que estaba en claro no pasa aquí: es una decisión del
    /// producto y la toma `obrain_api::cifrado::abrir`.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let path = path.as_ref();
        let key = if obrain_keys::exists(path) {
            Some(obrain_keys::unlock(path)?)
        } else {
            None
        };
        Self::open_with(path, key.as_ref())
    }

    /// Abre una base con esta llave, o en claro con `None`.
    pub fn open_with(
        path: impl AsRef<std::path::Path>,
        key: Option<&obrain_keys::DataKey>,
    ) -> Result<Self> {
        let conn = Connection::open(path)?;
        if let Some(k) = key {
            // La llave tiene que ser lo primero que se le dice a la conexión.
            // En crudo (`x'…'`) no hay derivación: son 256 bits de azar.
            conn.execute_batch(&format!(
                "PRAGMA key = \"{}\";",
                k.sqlcipher_literal().as_str()
            ))?;
            // SQLCipher no comprueba la llave hasta leer la primera página.
            // Se lee aquí para que una llave equivocada sea este error y no un
            // «file is not a database» tres llamadas más tarde.
            conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
                r.get::<_, i64>(0)
            })
            .map_err(|_| Error::WrongKey)?;
        }
        // Lo temporal, en memoria: un archivo temporal de SQLite en claro al
        // lado de una base cifrada sería una fuga por la puerta de atrás.
        conn.execute_batch("PRAGMA temp_store = MEMORY;")?;
        // WAL: la aplicación lee mientras el indexador escribe desde otra
        // conexión. Sin esto, cualquier lectura durante un reindexado fallaría
        // con «database is locked». Y la espera cubre a dos escritores que
        // coincidan, que en vez de fallar se turnan.
        conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get::<_, String>(0))?;
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        let store = Store { conn };
        store.migrate()?;
        Ok(store)
    }

    /// Copia una base **en claro** a `destino`, **cifrada** con esta llave.
    ///
    /// Es `sqlcipher_export`: esquema, datos, índices y triggers —los de I1 e
    /// I4 incluidos—. No toca el origen; sustituirlo es cosa de quien migra.
    pub fn export_encrypted(
        origen: impl AsRef<std::path::Path>,
        destino: impl AsRef<std::path::Path>,
        key: &obrain_keys::DataKey,
    ) -> Result<()> {
        let conn = Connection::open(origen)?;
        // Que el WAL quede volcado en el archivo principal antes de copiar.
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        conn.execute(
            "ATTACH DATABASE ?1 AS cifrada KEY ?2",
            rusqlite::params![
                destino.as_ref().to_string_lossy(),
                key.sqlcipher_literal().as_str()
            ],
        )?;
        conn.query_row("SELECT sqlcipher_export('cifrada')", [], |_| Ok(()))?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        conn.execute_batch(&format!("PRAGMA cifrada.user_version = {version};"))?;
        conn.execute_batch("DETACH DATABASE cifrada;")?;
        Ok(())
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(DDL)?;
        self.conn
            .pragma_update(None, "user_version", SCHEMA_VERSION)?;
        for (id, label, card, method, inverse) in SEED_PREDICATES {
            // La etiqueta es presentación y se actualiza; el resto de la
            // declaración no se toca en una base que ya existe.
            self.conn.execute(
                "INSERT INTO predicates
                     (id, label, cardinality, symmetric, transitive, inverse, default_method)
                 VALUES (?1, ?2, ?3, 0, 0, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET label = excluded.label",
                rusqlite::params![id, label, card, inverse, method],
            )?;
        }
        Ok(())
    }

    /// Acceso directo a la conexión.
    ///
    /// Es por donde se construyen las **proyecciones** —índice léxico FTS5,
    /// índice vectorial— que por I1 no son la verdad y viven en sus propias
    /// tablas. El log sigue siendo intocable: sus triggers no distinguen quién
    /// escribe.
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

    /// Recalcula la columna `sensitivity` desde las aserciones `sensitivity_level`
    /// vigentes y la escribe. Devuelve el nivel resultante.
    ///
    /// La columna sigue siendo **el punto de control**: ninguna comprobación de
    /// egreso resuelve el log, lee un índice. Esta función es el único puente
    /// entre ambos, y existe para que el control tenga procedencia (I9) y sea
    /// corregible (I5) sin dejar de ser un `SELECT` de una columna.
    ///
    /// Dos reglas, en este orden:
    ///
    /// 1. **Si el usuario se ha pronunciado, gana** — incluso para *bajar* el
    ///    nivel. Es incómodo escribirlo y es correcto: un clasificador
    ///    demasiado celoso que no se puede corregir convierte la bóveda en algo
    ///    que la gente apaga entera. La decisión queda en el log para siempre,
    ///    que es lo que la hace auditable en vez de invisible.
    /// 2. **Si no, gana la señal más restrictiva.** Entre "puede ser una clave"
    ///    y "parece normal", el sistema se equivoca hacia el lado que no filtra.
    ///
    /// Desde [ADR-0020] el usuario también se pronuncia **sobre una carpeta**, y
    /// esa marca la hereda todo lo que está dentro. Las reglas completas, con la
    /// herencia, están en [`Store::resolve_sensitivity`].
    ///
    /// Es una **proyección reconstruible** (I1): tirar la columna y volver a
    /// llamar aquí para cada entidad da exactamente el mismo estado.
    ///
    /// [ADR-0020]: ../../../docs/adr/0020-sensibilidad-por-carpeta.md
    pub fn reproject_sensitivity(&self, id: &EntityId, at: TimePoint) -> Result<Sensitivity> {
        let marks = self.folder_marks(at)?;
        self.reproject_sensitivity_under(id, at, &marks)
    }

    /// Cuántas entidades hay en cada nivel. Un control de seguridad tiene que
    /// poder mirarse de un vistazo.
    pub fn sensitivity_census(&self) -> Result<[i64; 3]> {
        let mut census = [0i64; 3];
        let mut stmt = self
            .conn
            .prepare("SELECT sensitivity, COUNT(*) FROM entities GROUP BY sensitivity")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        for row in rows {
            let (level, n) = row?;
            match Sensitivity::parse(&level) {
                Some(Sensitivity::Normal) => census[0] = n,
                Some(Sensitivity::Sensitive) => census[1] = n,
                Some(Sensitivity::Vault) => census[2] = n,
                None => return Err(Error::Corrupt(format!("sensibilidad '{level}'"))),
            }
        }
        Ok(census)
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

    /// Etiqueta legible de un predicado. El vocabulario es controlado (§3.4),
    /// así que la traducción de `contained_by` a "contenido en" vive con la
    /// definición y no en cada sitio que la enseñe.
    pub fn predicate_label(&self, predicate: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT label FROM predicates WHERE id = ?1",
                [predicate],
                |r| r.get(0),
            )
            .optional()?)
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
            &format!(
                "INSERT INTO assertions ({COLS}) VALUES \
                      (:id, :subject, :predicate, :kind, :ent, :txt, :num, \
                       :valid_from, :valid_to, :asserted_at, NULL, \
                       :method, :source, :confidence, :evidence)"
            ),
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

    /// Aserciones vigentes cuyo **objeto** es esta entidad.
    ///
    /// La contraria de [`Store::query`] con sujeto. Existe porque una arista se
    /// almacena **una sola vez** (§3.4: `contains` y `contained_by` son el mismo
    /// hecho), así que recorrer el grafo desde un nodo exige mirar en las dos
    /// direcciones: lo que él afirma y lo que se afirma de él. Sin esto, la
    /// lente de I2 sólo vería la mitad del vecindario — y sería justo la mitad
    /// equivocada, porque los nodos concentradores que creó M3 son objeto de sus
    /// aristas, no sujeto.
    ///
    /// Vive aquí y no en quien recorre el grafo por una razón concreta: el
    /// predicado bitemporal de I6 es una sola cadena de SQL en este archivo.
    /// Reescribirlo fuera sería tener dos definiciones de "vigente".
    pub fn query_by_object(&self, object: &EntityId, at: TimePoint) -> Result<Vec<Assertion>> {
        let sql = format!(
            "SELECT {COLS} FROM assertions
             WHERE object_entity = :object AND {BITEMPORAL}
             {PRECEDENCE}"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            named_params! {
                ":object": object.as_str(),
                ":valid":  at.valid,
                ":known":  at.known,
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

    /// Resuelve `(sujeto, predicado)` **sin filtrar por el eje de validez**:
    /// qué sostiene el sistema en `known` sobre esto, fuera cuando fuera cierto.
    ///
    /// No es un atajo para saltarse I6: es la otra pregunta que la
    /// bitemporalidad permite hacer, y hace falta siempre que el intervalo de
    /// validez sea **el dato que se busca** en vez de un filtro.
    ///
    /// El caso que la destapó: `session_span` guarda el intervalo de una sesión
    /// de trabajo en el eje de validez, que es lo correcto —la sesión ocurrió
    /// entonces, no ahora—. Consecuencia: [`Store::resolve`] con el presente
    /// **nunca** lo encuentra, porque `valid_to` ya pasó. Desde M3, todos los
    /// nodos de sesión se quedaron sin nombre legible y no se vio hasta que un
    /// mapa los dibujó como los objetos más grandes de la pantalla.
    ///
    /// Es también la primitiva que necesita M7: *«durante mi trámite de visa»*
    /// exige leer el intervalo de un `Process` sin conocerlo de antemano, que
    /// es exactamente esta consulta y no la otra.
    pub fn resolve_whenever(
        &self,
        subject: &EntityId,
        predicate: &str,
        known: i64,
    ) -> Result<Option<Assertion>> {
        let sql = format!(
            "SELECT {COLS} FROM assertions
             WHERE subject = :subject AND predicate = :predicate
               AND asserted_at <= :known
               AND (retracted_at IS NULL OR retracted_at > :known)
             {PRECEDENCE} LIMIT 1"
        );
        Ok(self
            .conn
            .query_row(
                &sql,
                named_params! {
                    ":subject":   subject.as_str(),
                    ":predicate": predicate,
                    ":known":     known,
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
            out.push(Contradiction {
                subject,
                predicate,
                assertions,
            });
        }
        Ok(out)
    }

    /// Busca la entidad que tiene `predicate = value` vigente.
    ///
    /// Es la proyección que permite reconocer un archivo ya visto: se busca
    /// por `file_id`, no por ruta (invariante I8).
    pub fn find_subject_by_value(
        &self,
        predicate: &str,
        value: &str,
        at: TimePoint,
    ) -> Result<Option<EntityId>> {
        let sql = format!(
            "SELECT subject FROM assertions
             WHERE predicate = :predicate AND object_text = :value AND {BITEMPORAL}
             {PRECEDENCE} LIMIT 1"
        );
        Ok(self
            .conn
            .query_row(
                &sql,
                named_params! {
                    ":predicate": predicate,
                    ":value":     value,
                    ":valid":     at.valid,
                    ":known":     at.known,
                },
                |r| Ok(EntityId(r.get(0)?)),
            )
            .optional()?)
    }

    /// La entidad cuyo `predicate` vale este instante, **sin filtrar por el eje
    /// de validez**.
    ///
    /// Es a [`Self::find_subject_by_value`] lo que [`Self::resolve_whenever`] es
    /// a [`Self::resolve`], y existe por el mismo motivo: un `session_span` vive
    /// en el eje de validez —la sesión *ocurrió* entonces— así que buscarlo con
    /// un `at.valid` de ahora no lo encuentra nunca. Sin esto, quien quiera
    /// saber si una sesión ya estaba escrita no puede preguntarlo, y la única
    /// salida es crearla otra vez.
    pub fn find_subject_by_timestamp_whenever(
        &self,
        predicate: &str,
        value: i64,
        known: i64,
    ) -> Result<Option<EntityId>> {
        let sql = "SELECT subject FROM assertions
             WHERE predicate = :predicate
               AND object_kind = 'timestamp'
               AND object_number = :value
               AND asserted_at <= :known
               AND (retracted_at IS NULL OR retracted_at > :known)
             LIMIT 1";
        Ok(self
            .conn
            .query_row(
                sql,
                named_params! {
                    ":predicate": predicate,
                    ":value":     value as f64,
                    ":known":     known,
                },
                |r| Ok(EntityId(r.get(0)?)),
            )
            .optional()?)
    }

    /// **Todas** las entidades que tienen `predicate = value` vigente.
    ///
    /// La versión en singular aplica precedencia y devuelve una. Ésta existe
    /// para el caso contrario: cuando que haya más de una *es* la información
    /// que se busca. Dos entidades vivas afirmando la misma ruta significan
    /// que una de las dos ya no está ahí.
    pub fn find_subjects_by_value(
        &self,
        predicate: &str,
        value: &str,
        at: TimePoint,
    ) -> Result<Vec<EntityId>> {
        let sql = format!(
            "SELECT DISTINCT subject FROM assertions
             WHERE predicate = :predicate AND object_text = :value AND {BITEMPORAL}
             ORDER BY subject"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            named_params! {
                ":predicate": predicate,
                ":value":     value,
                ":valid":     at.valid,
                ":known":     at.known,
            },
            |r| Ok(EntityId(r.get(0)?)),
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// ¿Existe ya esta aserción exacta y vigente?
    ///
    /// `resolve` no sirve para esto: en un predicado de cardinalidad `many`
    /// devuelve sólo la de mayor precedencia, no todas. Los enlazadores la
    /// necesitan para ser idempotentes.
    pub fn has_assertion(
        &self,
        subject: &EntityId,
        predicate: &str,
        object: &Value,
        at: TimePoint,
    ) -> Result<bool> {
        Ok(self
            .query(Some(subject), Some(predicate), at)?
            .iter()
            .any(|a| &a.object == object))
    }

    /// Ejecuta `f` dentro de una transacción. SQLite admite un solo escritor
    /// (ver `adr/0002`), así que la ingestión masiva **tiene** que agrupar.
    ///
    /// Genérica en el error a propósito. Los crates de arriba tienen sus
    /// propios errores —M4 tiene que poder hablar de ONNX Runtime y de
    /// tokenizadores, que a este núcleo no le constan— y obligarles a
    /// aplastarlos a [`Error`] para poder abrir una transacción sería cambiar
    /// diagnóstico por una firma más corta. `E: From<Error>` es todo lo que
    /// hace falta: la transacción sólo necesita poder informar de sus propios
    /// fallos de SQLite.
    pub fn transaction<T, E>(
        &mut self,
        f: impl FnOnce(&Store) -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E>
    where
        E: From<Error>,
    {
        self.conn
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| E::from(Error::from(e)))?;
        match f(self) {
            Ok(v) => {
                self.conn
                    .execute_batch("COMMIT")
                    .map_err(|e| E::from(Error::from(e)))?;
                Ok(v)
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
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
