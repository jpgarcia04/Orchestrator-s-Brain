//! La proyección léxica: `extractions`, `chunks` y el índice FTS5.
//!
//! **Nada de esto es la verdad.** El invariante I1 dice que el log de
//! aserciones es la fuente y todo índice es una proyección reconstruible; el
//! texto de un archivo es una *derivación* de ese archivo, no un hecho nuevo
//! sobre el mundo. Por eso el texto vive aquí, en tablas propias que se pueden
//! tirar y rehacer, y no dentro de aserciones inmutables. El razonamiento
//! completo está en `docs/adr/0005-donde-vive-el-texto-extraido.md`.
//!
//! Lo que sí va al log es el **hecho** de haber extraído: con qué extractor,
//! con qué chunker, cuánto texto salió y qué hash tenía. Eso es procedencia
//! (I9) y sobrevive a que se borre el índice.

use obrain_core::rusqlite::{Connection, OptionalExtension, params};
use obrain_core::{EntityId, Result};

/// Versión del **esquema** de la proyección, independiente del log.
/// Un índice se tira y se rehace; el log no se migra por esto.
pub const INDEX_VERSION: i64 = 1;

pub const DDL: &str = r#"
-- ───────────────────────────────────────────────── qué se extrajo de cada cosa
CREATE TABLE IF NOT EXISTS extractions (
    entity        TEXT NOT NULL REFERENCES entities(id),
    extractor     TEXT NOT NULL,
    extractor_ver INTEGER NOT NULL,
    -- I7: el chunker forma parte de la identidad de un fragmento. Está en la
    -- clave primaria para que dos chunkers puedan convivir durante una
    -- migración, que es exactamente lo que el invariante exige.
    chunker       TEXT NOT NULL,
    chunker_ver   INTEGER NOT NULL,
    status        TEXT NOT NULL,
    -- hash del ARCHIVO: si no cambió, no hay que volver a abrirlo
    file_hash     TEXT,
    -- hash del TEXTO NORMALIZADO: si no cambió, no hay que volver a fragmentar
    text_hash     TEXT,
    chars         INTEGER NOT NULL DEFAULT 0,
    chunk_count   INTEGER NOT NULL DEFAULT 0,
    extracted_at  INTEGER NOT NULL,
    detail        TEXT,
    PRIMARY KEY (entity, chunker, chunker_ver)
);

CREATE INDEX IF NOT EXISTS idx_extractions_status ON extractions(status);

-- ─────────────────────────────────────────────────────────────── fragmentos
-- `text` es exactamente la rebanada [byte_start, byte_end) del texto
-- normalizado. Esa igualdad es lo que hace que una cita apunte a un sitio real.
CREATE TABLE IF NOT EXISTS chunks (
    id          INTEGER PRIMARY KEY,
    entity      TEXT NOT NULL REFERENCES entities(id),
    chunker     TEXT NOT NULL,
    chunker_ver INTEGER NOT NULL,
    ordinal     INTEGER NOT NULL,
    byte_start  INTEGER NOT NULL,
    byte_end    INTEGER NOT NULL,
    text        TEXT NOT NULL,
    UNIQUE (entity, chunker, chunker_ver, ordinal)
);

CREATE INDEX IF NOT EXISTS idx_chunks_entity ON chunks(entity);

-- ────────────────────────────────────────────────────────────── indice FTS5
-- `remove_diacritics 2` no es cosmético: sin él, "matricula" no encuentra
-- "matrícula" y el índice es inútil en un corpus en español.
-- `content=` evita guardar el texto dos veces.
CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
    text,
    content='chunks',
    content_rowid='id',
    tokenize="unicode61 remove_diacritics 2"
);

CREATE TRIGGER IF NOT EXISTS chunks_ai AFTER INSERT ON chunks BEGIN
    INSERT INTO chunks_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER IF NOT EXISTS chunks_ad AFTER DELETE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;
CREATE TRIGGER IF NOT EXISTS chunks_au AFTER UPDATE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES ('delete', old.id, old.text);
    INSERT INTO chunks_fts(rowid, text) VALUES (new.id, new.text);
END;
"#;

/// Crea la proyección si no existe. Idempotente.
pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(DDL)?;
    Ok(())
}

/// Lo que ya sabíamos de una entidad, para no repetir trabajo.
#[derive(Debug, Clone)]
pub struct Previous {
    /// Con qué se extrajo: `pdf`, `pdf+ocr`, `image`… Es lo que dice si a un
    /// PDF escaneado ya se le pasó el OCR o todavía no.
    pub extractor: String,
    pub extractor_ver: i64,
    pub file_hash: Option<String>,
    pub status: String,
}

pub fn previous(
    conn: &Connection,
    entity: &EntityId,
    chunker: &str,
    chunker_ver: i64,
) -> Result<Option<Previous>> {
    Ok(conn
        .query_row(
            "SELECT extractor_ver, file_hash, status, extractor FROM extractions
             WHERE entity = ?1 AND chunker = ?2 AND chunker_ver = ?3",
            params![entity.as_str(), chunker, chunker_ver],
            |r| {
                Ok(Previous {
                    extractor_ver: r.get(0)?,
                    file_hash: r.get(1)?,
                    status: r.get(2)?,
                    extractor: r.get(3)?,
                })
            },
        )
        .optional()?)
}

/// Una fila de `extractions` a punto de escribirse.
pub struct Record<'a> {
    pub entity: &'a EntityId,
    pub extractor: &'a str,
    pub extractor_ver: i64,
    pub chunker: &'a str,
    pub chunker_ver: i64,
    pub status: &'a str,
    pub file_hash: Option<&'a str>,
    pub text_hash: Option<&'a str>,
    pub chars: usize,
    pub chunk_count: usize,
    pub at: i64,
    pub detail: Option<&'a str>,
}

pub fn record(conn: &Connection, r: Record<'_>) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO extractions
             (entity, extractor, extractor_ver, chunker, chunker_ver, status,
              file_hash, text_hash, chars, chunk_count, extracted_at, detail)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        params![
            r.entity.as_str(),
            r.extractor,
            r.extractor_ver,
            r.chunker,
            r.chunker_ver,
            r.status,
            r.file_hash,
            r.text_hash,
            r.chars as i64,
            r.chunk_count as i64,
            r.at,
            r.detail,
        ],
    )?;
    Ok(())
}

/// Sustituye los fragmentos de una entidad para un chunker dado.
///
/// Borrar y reinsertar, no acumular: reextraer un archivo editado tiene que
/// dejar el índice describiendo el archivo de hoy. Que el log sea append-only
/// (I4) no obliga a que lo sea una proyección — al contrario, es lo que hace
/// que la proyección pueda seguir a la realidad sin reescribir la historia.
pub fn replace_chunks(
    conn: &Connection,
    entity: &EntityId,
    chunker: &str,
    chunker_ver: i64,
    chunks: &[crate::chunk::Chunk],
) -> Result<()> {
    conn.execute(
        "DELETE FROM chunks WHERE entity = ?1 AND chunker = ?2 AND chunker_ver = ?3",
        params![entity.as_str(), chunker, chunker_ver],
    )?;
    let mut stmt = conn.prepare_cached(
        "INSERT INTO chunks (entity, chunker, chunker_ver, ordinal, byte_start, byte_end, text)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
    )?;
    for c in chunks {
        stmt.execute(params![
            entity.as_str(),
            chunker,
            chunker_ver,
            c.ordinal as i64,
            c.byte_start as i64,
            c.byte_end as i64,
            c.text,
        ])?;
    }
    Ok(())
}

/// Cuenta de fragmentos vivos en el índice.
pub fn chunk_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?)
}

/// Caracteres almacenados en los fragmentos, **contando el solape**.
///
/// Comparado con los caracteres del texto original da el coste real de la
/// estrategia de fragmentación, que es un número que conviene mirar antes de
/// que el corpus tenga 200.000 archivos.
pub fn stored_chars(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(LENGTH(text)), 0) FROM chunks",
        [],
        |r| r.get(0),
    )?)
}

/// Los archivos que más ocupan el índice, de mayor a menor.
///
/// Es la consulta que destapa la contaminación: si los diez primeros son
/// bundles minificados y volcados de datos, el índice describe la basura del
/// disco y no lo que el usuario sabe. Los contadores globales jamás lo dicen.
pub fn heaviest(conn: &Connection, limit: usize) -> Result<Vec<(EntityId, i64, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT entity, chunk_count, chars FROM extractions
         WHERE status = 'ok' ORDER BY chunk_count DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit as i64], |r| {
        Ok((EntityId(r.get(0)?), r.get(1)?, r.get(2)?))
    })?;
    Ok(rows.collect::<obrain_core::rusqlite::Result<Vec<_>>>()?)
}

/// Entidades cuya extracción acabó en un estado concreto.
pub fn with_status(conn: &Connection, status: &str, limit: usize) -> Result<Vec<EntityId>> {
    let mut stmt =
        conn.prepare("SELECT entity FROM extractions WHERE status = ?1 ORDER BY entity LIMIT ?2")?;
    let rows = stmt.query_map(params![status, limit as i64], |r| Ok(EntityId(r.get(0)?)))?;
    Ok(rows.collect::<obrain_core::rusqlite::Result<Vec<_>>>()?)
}

/// Fragmentos de una entidad, en orden.
pub fn chunks_of(conn: &Connection, entity: &EntityId) -> Result<Vec<(i64, String)>> {
    let mut stmt =
        conn.prepare("SELECT ordinal, text FROM chunks WHERE entity = ?1 ORDER BY ordinal")?;
    let rows = stmt.query_map([entity.as_str()], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<obrain_core::rusqlite::Result<Vec<_>>>()?)
}

/// El texto entero de una entidad, recompuesto desde sus fragmentos.
///
/// **Concatenar los fragmentos no vale**: `paragraph-pack@1` deja 200 bytes de
/// solape, así que pegarlos repetiría el final de cada uno al principio del
/// siguiente. Se empalma por offset, y eso se puede hacer *exactamente* porque
/// `text` **es** la rebanada `[byte_start, byte_end)` del documento y no una
/// copia parecida — la misma igualdad de la que vive
/// [`crate::evidence`](../evidence/index.html).
///
/// No abre ningún archivo: sale de la proyección que M2 escribió
/// ([ADR-0005]). Es lo que hace que previsualizar texto no pueda hidratar un
/// puntero de OneDrive ni por descuido, que es la trampa de M5b.
///
/// [ADR-0005]: ../../../docs/adr/0005-donde-vive-el-texto-extraido.md
pub fn document_text(conn: &Connection, entity: &EntityId) -> Result<Option<String>> {
    let mut stmt =
        conn.prepare("SELECT byte_start, text FROM chunks WHERE entity = ?1 ORDER BY ordinal")?;
    let rows = stmt.query_map([entity.as_str()], |r| {
        Ok((r.get::<_, i64>(0)?.max(0) as usize, r.get::<_, String>(1)?))
    })?;

    let mut costura = Costura::default();
    for row in rows {
        let (start, text) = row?;
        costura.poner(start, &text);
    }
    Ok(costura.hubo.then_some(costura.out))
}

/// Los textos de **todas** las entidades, de una pasada, uno cada vez.
///
/// Es [`document_text`] para quien tiene que leerlo todo —comparar cada
/// documento con los demás ([ADR-0030])—: una consulta en vez de una por
/// documento, y un texto en memoria cada vez en vez de todos a la vez. Se
/// empalma con la misma costura, así que los bytes son los mismos que enseña
/// la vista previa y un tramo apunta al mismo sitio en los dos.
///
/// [ADR-0030]: ../../../docs/adr/0030-un-documento-dentro-de-otro.md
pub fn each_document_text(
    conn: &Connection,
    mut f: impl FnMut(EntityId, &str) -> Result<()>,
) -> Result<()> {
    let mut stmt =
        conn.prepare("SELECT entity, byte_start, text FROM chunks ORDER BY entity, ordinal")?;
    let mut rows = stmt.query([])?;
    let mut actual: Option<(String, Costura)> = None;
    while let Some(r) = rows.next()? {
        let entity: String = r.get(0)?;
        let start = r.get::<_, i64>(1)?.max(0) as usize;
        let text: String = r.get(2)?;
        if actual.as_ref().is_none_or(|(e, _)| *e != entity) {
            if let Some((e, c)) = actual.take() {
                f(EntityId(e), &c.out)?;
            }
            actual = Some((entity, Costura::default()));
        }
        if let Some((_, c)) = actual.as_mut() {
            c.poner(start, &text);
        }
    }
    if let Some((e, c)) = actual {
        f(EntityId(e), &c.out)?;
    }
    Ok(())
}

/// Empalma los fragmentos de un documento, en orden, por su offset.
#[derive(Default)]
struct Costura {
    out: String,
    cursor: usize,
    hubo: bool,
}

impl Costura {
    fn poner(&mut self, start: usize, text: &str) {
        self.hubo = true;
        let end = start + text.len();
        if end <= self.cursor && !self.out.is_empty() {
            return; // enteramente dentro de lo que ya está puesto
        }
        let desde = self.cursor.saturating_sub(start);
        if desde == 0 {
            // Un hueco entre fragmentos no debería existir. Si lo hubiera, se
            // marca con un salto en vez de pegar dos párrafos que no iban
            // juntos: un texto mal cosido se lee como un archivo corrupto.
            if start > self.cursor && !self.out.is_empty() {
                self.out.push('\n');
            }
            self.out.push_str(text);
        } else if desde < text.len() && text.is_char_boundary(desde) {
            self.out.push_str(&text[desde..]);
        } else if desde < text.len() {
            self.out.push('\n');
            self.out.push_str(text);
        }
        self.cursor = self.cursor.max(end);
    }
}
