//! El log de aserciones y sus proyecciones.
//!
//! Toda escritura es un `INSERT`. La única mutación permitida en todo el
//! sistema es fijar `retracted_at`, y los triggers de `schema.rs` garantizan
//! que no hay otra.

use crate::error::{Error, Result};
use crate::model::*;
use crate::opening;
use crate::schema::{DDL, SCHEMA_VERSION, SEED_PREDICATES};
use rusqlite::{Connection, OptionalExtension, named_params, params};

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
        if !path.exists() && opening::has_artifacts(path, true) {
            return Err(opening::suspect(
                "hay llave o archivos auxiliares pero falta la base",
            ));
        }
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
        let path = path.as_ref();
        if !path.exists() {
            if key.is_some() || opening::has_artifacts(path, true) {
                return Err(opening::suspect(
                    "falta la base existente; no se reutiliza su llave para crear otra",
                ));
            }
            return Self::create_with(path, None);
        }
        Self::verify_existing(path, key)?;
        // Sin CREATE: si desapareció después de verificarla, no se recrea.
        let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        opening::key(&conn, key)?;
        // La copia previa no es un respaldo consistente de una base viva.
        // Revalidamos el estado real antes de ejecutar DDL o migraciones.
        opening::validate(&conn)?;
        Self::initialize(conn)
    }

    /// Creación explícita. Nunca sobrescribe una base ni restos de un WAL.
    /// La llave puede haberse escrito justo antes; el llamante debe poseerla.
    pub fn create_with(
        path: impl AsRef<std::path::Path>,
        key: Option<&obrain_keys::DataKey>,
    ) -> Result<Self> {
        let path = path.as_ref();
        if opening::has_artifacts(path, false)
            || (key.is_none() && obrain_keys::key_path(path).exists())
        {
            return Err(opening::suspect(
                "hay restos de una base anterior en la ruta de creación",
            ));
        }
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        opening::key(&conn, key)?;
        Self::initialize(conn)
    }

    /// Verifica una base existente sin escribir en ella ni en sus auxiliares.
    /// Sirve también antes de cifrar o de volver a envolver su llave.
    pub fn verify_existing(
        path: impl AsRef<std::path::Path>,
        key: Option<&obrain_keys::DataKey>,
    ) -> Result<()> {
        opening::verify(path.as_ref(), key)
    }

    /// Antes de generar una llave nueva, exige una ruta sin base ni restos.
    pub fn check_new_path(path: impl AsRef<std::path::Path>) -> Result<()> {
        let path = path.as_ref();
        if path.exists() || opening::has_artifacts(path, true) {
            return Err(opening::suspect(
                "la ruta contiene una base, llave o auxiliares anteriores",
            ));
        }
        Ok(())
    }

    fn initialize(conn: Connection) -> Result<Self> {
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
    /// I4 incluidos—. Consolida el WAL del origen; sustituirlo es cosa de quien migra.
    pub fn export_encrypted(
        origen: impl AsRef<std::path::Path>,
        destino: impl AsRef<std::path::Path>,
        key: &obrain_keys::DataKey,
    ) -> Result<()> {
        Self::verify_existing(origen.as_ref(), None)?;
        if destino.as_ref().exists() || opening::has_artifacts(destino.as_ref(), false) {
            return Err(opening::suspect("el destino de cifrado ya existe"));
        }
        let conn =
            Connection::open_with_flags(origen, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        opening::validate(&conn)?;
        // Que el WAL quede volcado en el archivo principal antes de copiar.
        Self::checkpoint(&conn)?;
        // ATTACH hereda la apertura sin CREATE del origen. Reservamos sólo el
        // destino, después de validar el origen, sin sobrescribir otro archivo.
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destino.as_ref())?;
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

    /// No se puede mover sólo el principal si el checkpoint quedó ocupado.
    pub fn checkpoint(conn: &Connection) -> Result<()> {
        let (busy, frames, copied): (i64, i64, i64) =
            conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?;
        if busy != 0 || (frames >= 0 && frames != copied) {
            return Err(opening::suspect(
                "el WAL sigue en uso; cierra las otras conexiones y reintenta",
            ));
        }
        Ok(())
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(DDL)?;
        self.atomic_write(|store| {
            // También repara una migración interrumpida después de crear la
            // tabla: una ruta viva sin registro queda en cuarentena.
            store.conn.execute(
                "INSERT OR IGNORE INTO content_publication (entity, state, origin)
                 SELECT DISTINCT subject, 'pending', 'legacy' FROM assertions
                  WHERE predicate = 'path_observed_at' AND retracted_at IS NULL",
                [],
            )?;
            store.conn.execute(
                "UPDATE content_publication SET observed_hash = (
                    SELECT object_text FROM assertions a
                     WHERE a.subject=content_publication.entity
                       AND a.predicate='has_hash' AND a.retracted_at IS NULL
                     ORDER BY a.asserted_at DESC LIMIT 1
                 ) WHERE state='pending' AND origin='legacy' AND observed_hash IS NULL",
                [],
            )?;
            let version: i32 = store
                .conn
                .pragma_query_value(None, "user_version", |r| r.get(0))?;
            if version < SCHEMA_VERSION {
                let integrity: String = store
                    .conn
                    .query_row("PRAGMA quick_check", [], |r| r.get(0))?;
                if integrity != "ok" {
                    return Err(Error::Corrupt(format!(
                        "migración: quick_check: {integrity}"
                    )));
                }
                let foreign_keys: i64 = store.conn.query_row(
                    "SELECT COUNT(*) FROM pragma_foreign_key_check",
                    [],
                    |r| r.get(0),
                )?;
                if foreign_keys != 0 {
                    return Err(Error::Corrupt(format!(
                        "migración: {foreign_keys} referencias inválidas"
                    )));
                }
                store
                    .conn
                    .pragma_update(None, "user_version", SCHEMA_VERSION)?;
            }
            for (id, label, card, method, inverse) in SEED_PREDICATES {
                // La etiqueta es presentación y se actualiza; el resto de la
                // declaración no se toca en una base que ya existe.
                store.conn.execute(
                    "INSERT INTO predicates
                         (id, label, cardinality, symmetric, transitive, inverse, default_method)
                     VALUES (?1, ?2, ?3, 0, 0, ?4, ?5)
                     ON CONFLICT(id) DO UPDATE SET label = excluded.label",
                    rusqlite::params![id, label, card, inverse, method],
                )?;
            }
            Ok(())
        })
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

    /// Una lectura que consulta el control y después el contenido debe ver una
    /// sola instantánea WAL, incluso si otra conexión confirma una indexación.
    pub fn read_snapshot<T, E>(
        &self,
        f: impl FnOnce(&Store) -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E>
    where
        E: From<Error>,
    {
        self.conn
            .execute_batch("BEGIN DEFERRED")
            .map_err(Error::from)?;
        match f(self) {
            Ok(value) => match self.conn.execute_batch("COMMIT") {
                Ok(()) => Ok(value),
                Err(error) => {
                    let _ = self.conn.execute_batch("ROLLBACK");
                    Err(Error::from(error).into())
                }
            },
            Err(error) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    /// Si esta entidad tiene una versión de contenido todavía no publicable.
    /// Las entidades sin fila son carpetas, sesiones o fixtures sin ingesta.
    pub fn content_pending(&self, id: &EntityId) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM content_publication WHERE entity=?1 AND state='pending')",
            [id.as_str()],
            |r| r.get(0),
        )?)
    }

    pub fn content_pending_set(&self) -> Result<std::collections::HashSet<EntityId>> {
        let mut stmt = self
            .conn
            .prepare("SELECT entity FROM content_publication WHERE state='pending'")?;
        let rows = stmt.query_map([], |r| Ok(EntityId(r.get(0)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn legacy_publication_pending(&self) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM content_publication WHERE state='pending' AND origin='legacy')",
            [], |r| r.get(0),
        )?)
    }

    /// Se llama en la misma transacción que ingesta una versión nueva. La
    /// primera preparación conserva los chunks publicados previamente.
    pub fn prepare_content(&self, id: &EntityId, observed_hash: Option<&str>) -> Result<()> {
        let captured: Option<i64> = self
            .conn
            .query_row(
                "SELECT backup_captured FROM content_publication WHERE entity=?1",
                [id.as_str()],
                |r| r.get(0),
            )
            .optional()?;
        if captured != Some(1) {
            let chunks_exist: bool = self.conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='chunks')",
                [],
                |r| r.get(0),
            )?;
            if chunks_exist {
                self.conn.execute(
                    "INSERT OR IGNORE INTO publication_backup_chunks
                     (entity, chunker, chunker_ver, ordinal, byte_start, byte_end, text)
                     SELECT entity, chunker, chunker_ver, ordinal, byte_start, byte_end, text
                       FROM chunks WHERE entity=?1",
                    [id.as_str()],
                )?;
            }
        }
        self.conn.execute(
            "INSERT INTO content_publication
             (entity, state, origin, observed_hash, backup_captured)
             VALUES (?1, 'pending', 'index', ?2, 1)
             ON CONFLICT(entity) DO UPDATE SET
                state='pending', origin='index', observed_hash=excluded.observed_hash,
                backup_captured=1",
            params![id.as_str(), observed_hash],
        )?;
        Ok(())
    }

    /// Publica sólo la versión que se clasificó. Un segundo indexador que haya
    /// ingresado otra observación entre etapas impide este cambio.
    pub fn publish_content(&self, id: &EntityId, expected_hash: Option<&str>) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE content_publication SET state='ready', origin='index',
                    published_hash=observed_hash, backup_captured=0
              WHERE entity=?1 AND state='pending' AND observed_hash IS ?2",
            params![id.as_str(), expected_hash],
        )?;
        if changed != 0 {
            self.conn.execute(
                "DELETE FROM publication_backup_chunks WHERE entity=?1",
                [id.as_str()],
            )?;
        }
        Ok(changed != 0)
    }

    /// Hash de los bytes que pueden presentarse como miniatura. `None` falla
    /// cerrado para imágenes sin hash comprobable.
    pub fn published_content_hash(&self, id: &EntityId) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT published_hash FROM content_publication
              WHERE entity=?1 AND state='ready'",
                [id.as_str()],
                |r| r.get(0),
            )
            .optional()?
            .flatten())
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

    /// Escribe el control de seguridad y revoca el estado de embeddings al
    /// entrar o salir de `vault`, en el mismo savepoint y con el tiempo del llamante.
    /// También reconcilia un `vault` ya proyectado con residuos anteriores.
    pub fn set_sensitivity(&self, id: &EntityId, s: Sensitivity, at: i64) -> Result<()> {
        self.atomic_write(|store| {
            let previous = store.sensitivity(id)?;
            let n = store.conn.execute(
                "UPDATE entities SET sensitivity = ?2 WHERE id = ?1",
                rusqlite::params![id.as_str(), s.as_str()],
            )?;
            if n == 0 {
                return Err(Error::UnknownEntity(id.to_string()));
            }
            if s == Sensitivity::Vault || previous == Sensitivity::Vault {
                store.revoke_embeddings(id, at)?;
            }
            Ok(())
        })
    }

    /// Revoca la proyección opcional y su procedencia actual. No carga un
    /// modelo ni crea tablas de M4 en un Store que sólo tiene el esquema core.
    /// El historial permanece consultable con un `known` anterior a `at`.
    fn revoke_embeddings(&self, id: &EntityId, at: i64) -> Result<bool> {
        let has_vectors: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'vectors')",
            [],
            |r| r.get(0),
        )?;
        let vectors = if has_vectors {
            self.conn
                .execute("DELETE FROM vectors WHERE entity = ?1", [id.as_str()])?
        } else {
            0
        };
        let claims = self.conn.execute(
            "UPDATE assertions SET retracted_at = ?2 WHERE subject = ?1
             AND predicate IN ('embedded_with', 'vector_count')
             AND retracted_at IS NULL AND asserted_at <= ?2",
            rusqlite::params![id.as_str(), at],
        )?;
        Ok(vectors > 0 || claims > 0)
    }

    /// Reconcilia residuos de versiones anteriores, incluso procedencia sin
    /// vectores. Se usa también al clasificar sin modelo. No sanea el historial.
    pub fn purge_vault_embeddings(&self, at: i64) -> Result<Vec<EntityId>> {
        self.atomic_write(|store| {
            let ids: Vec<EntityId> = store
                .conn
                .prepare("SELECT id FROM entities WHERE sensitivity = 'vault' ORDER BY id")?
                .query_map([], |r| Ok(EntityId(r.get(0)?)))?
                .collect::<rusqlite::Result<_>>()?;
            let mut purged = Vec::new();
            for id in ids {
                if store.revoke_embeddings(&id, at)? {
                    purged.push(id);
                }
            }
            Ok(purged)
        })
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

    /// Escribe una respuesta del usuario (I5) **sin repetirla**.
    ///
    /// Si lo último que dijo sobre `(sujeto, predicado)` ya era esto, no
    /// escribe nada y devuelve aquella aserción: pulsar «es correcto» dos veces
    /// es una respuesta, no dos. En cardinalidad `one`, lo que dijo antes con
    /// otro valor deja de estar vigente —se retracta, no se borra (I4)—: cambió
    /// de opinión, y el log lo cuenta. Lo inferido no se toca nunca: la
    /// precedencia ya hace que mande lo suyo.
    pub fn assert_by_user(&self, a: NewAssertion) -> Result<AssertionId> {
        if a.method != Method::User {
            return Err(Error::Corrupt("assert_by_user exige método user".into()));
        }
        self.atomic_write(|s| s.assert_user_inner(a))
    }

    fn assert_user_inner(&self, a: NewAssertion) -> Result<AssertionId> {
        let suyas: Vec<Assertion> = self
            .query(
                Some(&a.subject),
                Some(&a.predicate),
                TimePoint::at(a.asserted_at),
            )?
            .into_iter()
            .filter(|x| x.method == Method::User)
            .collect();
        // `query` ordena por precedencia: la primera es la más reciente.
        if let Some(ultima) = suyas.first().filter(|x| x.object == a.object) {
            return Ok(ultima.id.clone());
        }
        // Las señales de sensibilidad son many, pero la decisión del usuario
        // es única: dos cambios en el mismo milisegundo no pueden dejar que
        // mande la respuesta anterior por el orden accidental de SQLite.
        if a.predicate == "sensitivity_level" || self.cardinality(&a.predicate)? == Cardinality::One
        {
            for x in &suyas {
                self.retract(&x.id, a.asserted_at)?;
            }
        }
        self.assert(a)
    }

    /// Agrupa una escritura de dominio, también dentro de una transacción de
    /// etapa. Un fallo no deja retractaciones ni proyecciones a medias.
    pub fn atomic_write<T, E>(
        &self,
        f: impl FnOnce(&Store) -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E>
    where
        E: From<Error>,
    {
        self.conn
            .execute_batch("SAVEPOINT domain_write")
            .map_err(|e| E::from(Error::from(e)))?;
        let result = f(self);
        match result {
            Ok(value) => {
                if let Err(e) = self.conn.execute_batch("RELEASE domain_write") {
                    let _ = self
                        .conn
                        .execute_batch("ROLLBACK TO domain_write; RELEASE domain_write");
                    return Err(E::from(Error::from(e)));
                }
                Ok(value)
            }
            Err(e) => {
                let _ = self
                    .conn
                    .execute_batch("ROLLBACK TO domain_write; RELEASE domain_write");
                Err(e)
            }
        }
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
        self.pointing_at(object, None, at)
    }

    /// [`Store::query_by_object`] de un solo predicado: *qué documentos están
    /// dentro de éste* sin traerse, de paso, los cientos de `contained_by` de
    /// una carpeta.
    pub fn pointing_at(
        &self,
        object: &EntityId,
        predicate: Option<&str>,
        at: TimePoint,
    ) -> Result<Vec<Assertion>> {
        let sql = format!(
            "SELECT {COLS} FROM assertions
             WHERE object_entity = :object
               AND (:predicate IS NULL OR predicate = :predicate)
               AND {BITEMPORAL}
             {PRECEDENCE}"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            named_params! {
                ":object":    object.as_str(),
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

    /// Lo **último** que se afirmó de `(sujeto, predicado)` hasta `known`,
    /// **aunque ya esté retractado**.
    ///
    /// Es la pregunta de la historia, no la del presente: *«¿dónde estaba?»*.
    /// La destapó la papelera ([ADR-0025]): un documento que ya no está tiene
    /// su ruta retractada —ya no vive ahí— y aun así hay que poder llamarlo por
    /// su nombre y decir en qué carpeta estaba.
    ///
    /// [ADR-0025]: ../../../docs/adr/0025-lo-que-ya-no-esta.md
    pub fn resolve_last(
        &self,
        subject: &EntityId,
        predicate: &str,
        known: i64,
    ) -> Result<Option<Assertion>> {
        let sql = format!(
            "SELECT {COLS} FROM assertions
             WHERE subject = :subject AND predicate = :predicate
               AND asserted_at <= :known
             ORDER BY asserted_at DESC, id DESC LIMIT 1"
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
