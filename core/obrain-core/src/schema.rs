//! Esquema SQLite. Implementa `docs/adr/0002-sustrato-de-persistencia.md`.
//!
//! Lo importante de este módulo no son las tablas: son los **triggers**.
//! Los invariantes I1 (log inmutable) e I4 (nada se borra) se imponen en el
//! motor, no por convención. Ningún código futuro — ni un `DELETE` a mano en
//! una consola de SQLite — puede violarlos.

pub const SCHEMA_VERSION: i32 = 1;

pub const DDL: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- ─────────────────────────────────────────────────────────────── entidades
CREATE TABLE IF NOT EXISTS entities (
    id           TEXT PRIMARY KEY,
    created_at   INTEGER NOT NULL,
    -- Única excepción a "todo es una aserción": un control de seguridad no
    -- puede depender de resolver una proyección. Ver model::Sensitivity.
    sensitivity  TEXT NOT NULL DEFAULT 'normal'
                 CHECK (sensitivity IN ('normal','sensitive','vault'))
);

-- ────────────────────────────────────────────────────────────── predicados
CREATE TABLE IF NOT EXISTS predicates (
    id              TEXT PRIMARY KEY,
    label           TEXT NOT NULL,
    cardinality     TEXT NOT NULL CHECK (cardinality IN ('one','many')),
    symmetric       INTEGER NOT NULL DEFAULT 0,
    transitive      INTEGER NOT NULL DEFAULT 0,
    inverse         TEXT,
    default_method  TEXT NOT NULL
                    CHECK (default_method IN ('inferred','imported','deterministic','user'))
);

-- ────────────────────────────────────────────────────────────── aserciones
-- Append-only. La fuente de verdad (invariante I1).
CREATE TABLE IF NOT EXISTS assertions (
    id             TEXT PRIMARY KEY,
    subject        TEXT NOT NULL REFERENCES entities(id),
    predicate      TEXT NOT NULL REFERENCES predicates(id),

    object_kind    TEXT NOT NULL
                   CHECK (object_kind IN ('entity','text','number','timestamp','bool')),
    object_entity  TEXT REFERENCES entities(id),
    object_text    TEXT,
    object_number  REAL,

    -- bitemporalidad (I6)
    valid_from     INTEGER,
    valid_to       INTEGER,
    asserted_at    INTEGER NOT NULL,
    retracted_at   INTEGER,

    -- procedencia (I9)
    method         TEXT NOT NULL
                   CHECK (method IN ('inferred','imported','deterministic','user')),
    source         TEXT NOT NULL DEFAULT '',
    confidence     REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    evidence       TEXT,

    -- un intervalo de validez invertido no significa nada
    CHECK (valid_from IS NULL OR valid_to IS NULL OR valid_from < valid_to),
    -- no se puede retractar algo antes de afirmarlo
    CHECK (retracted_at IS NULL OR retracted_at >= asserted_at)
);

CREATE INDEX IF NOT EXISTS idx_assert_sp        ON assertions(subject, predicate);
CREATE INDEX IF NOT EXISTS idx_assert_asserted  ON assertions(asserted_at);
CREATE INDEX IF NOT EXISTS idx_assert_object    ON assertions(object_entity)
                                                WHERE object_entity IS NOT NULL;
-- índice parcial: la inmensa mayoría de las consultas sólo miran lo vigente
CREATE INDEX IF NOT EXISTS idx_assert_live      ON assertions(subject, predicate)
                                                WHERE retracted_at IS NULL;

-- ═══════════════════════════════════════════════════════════════ INVARIANTES
-- Impuestos por el motor. No son convenciones: son imposibilidades.

-- I4 · nada se borra, se retracta
CREATE TRIGGER IF NOT EXISTS assertions_no_delete
BEFORE DELETE ON assertions
BEGIN
    SELECT RAISE(ABORT, 'I4: las aserciones no se borran, se retractan');
END;

CREATE TRIGGER IF NOT EXISTS entities_no_delete
BEFORE DELETE ON entities
BEGIN
    SELECT RAISE(ABORT, 'I4: las entidades no se borran');
END;

-- I1 · el log es inmutable; sólo retracted_at puede cambiar.
-- `IS NOT` en vez de `<>` para que la comparación sea segura con NULL.
CREATE TRIGGER IF NOT EXISTS assertions_immutable
BEFORE UPDATE ON assertions
WHEN OLD.id            IS NOT NEW.id
  OR OLD.subject       IS NOT NEW.subject
  OR OLD.predicate     IS NOT NEW.predicate
  OR OLD.object_kind   IS NOT NEW.object_kind
  OR OLD.object_entity IS NOT NEW.object_entity
  OR OLD.object_text   IS NOT NEW.object_text
  OR OLD.object_number IS NOT NEW.object_number
  OR OLD.valid_from    IS NOT NEW.valid_from
  OR OLD.valid_to      IS NOT NEW.valid_to
  OR OLD.asserted_at   IS NOT NEW.asserted_at
  OR OLD.method        IS NOT NEW.method
  OR OLD.source        IS NOT NEW.source
  OR OLD.confidence    IS NOT NEW.confidence
  OR OLD.evidence      IS NOT NEW.evidence
BEGIN
    SELECT RAISE(ABORT, 'I1: el log de aserciones es inmutable; solo retracted_at puede cambiar');
END;

-- La retractación es definitiva: no se reescribe la historia.
CREATE TRIGGER IF NOT EXISTS assertions_retract_once
BEFORE UPDATE OF retracted_at ON assertions
WHEN OLD.retracted_at IS NOT NULL AND NEW.retracted_at IS NOT OLD.retracted_at
BEGIN
    SELECT RAISE(ABORT, 'I4: la retraccion es definitiva; esta asercion ya estaba retractada');
END;
"#;

/// Vocabulario inicial de predicados (`PROJECT-MASTER.md` §3.4).
///
/// Agrupado por **cómo se obtiene**, que es lo que determina su coste y su
/// fiabilidad. Los tres primeros grupos no necesitan IA y son el 80% del valor
/// inicial del grafo.
pub const SEED_PREDICATES: &[(&str, &str, &str, &str, Option<&str>)] = &[
    // (id, label, cardinalidad, método por defecto, inverso)

    // ── estructural · determinista ──
    ("contains", "contiene", "many", "deterministic", Some("contained_by")),
    ("contained_by", "contenido en", "one", "deterministic", Some("contains")),
    ("derived_from", "derivado de", "one", "deterministic", None),
    ("version_of", "versión de", "one", "deterministic", None),
    ("duplicate_of", "duplicado de", "many", "deterministic", None),
    ("attached_to", "adjunto a", "one", "deterministic", None),
    ("downloaded_from", "descargado de", "one", "deterministic", None),
    ("has_hash", "hash", "one", "deterministic", None),
    ("path_observed_at", "ruta observada", "many", "deterministic", None),
    ("has_facet", "tiene faceta", "many", "deterministic", None),
    // ── sistema de archivos · determinista (M1) ──
    ("file_id", "identidad de archivo", "one", "deterministic", None),
    ("file_size", "tamaño", "one", "deterministic", None),
    ("modified_at", "modificado", "one", "deterministic", None),
    ("availability", "disponibilidad del contenido", "one", "deterministic", None),
    ("mime_guess", "tipo estimado", "one", "deterministic", None),
    // ── enlazadores deterministas (M3) ──
    ("folder_path", "ruta de carpeta", "one", "deterministic", None),
    ("session_span", "intervalo de la sesión", "one", "deterministic", None),
    // ── extraccion de contenido (M2) ──
    ("extraction_status", "resultado de la extracción", "one", "deterministic", None),
    ("text_chars", "caracteres de texto", "one", "deterministic", None),
    ("text_hash", "hash del texto extraido", "one", "deterministic", None),
    ("chunk_count", "número de fragmentos", "one", "deterministic", None),
    // El chunker vive en el log, no sólo en la proyección: el invariante I7
    // exige que todo vector sepa con qué chunker se produjo su fragmento.
    ("chunker", "estrategia de fragmentación", "one", "deterministic", None),
    // ── vectorizacion (M4) ──
    // La procedencia del índice vectorial, no el índice. El vector vive en la
    // tabla `vectors`, que es una proyección; lo que no se puede recalcular
    // mirándola es **con qué se produjo**, y eso sí es un hecho (I7, I9).
    ("embedded_with", "vectorizado con", "one", "deterministic", None),
    ("vector_count", "número de vectores", "one", "deterministic", None),
    // ── sensibilidad · procedencia del control de egreso (I11) ──
    // La columna `entities.sensitivity` sigue siendo el punto de control. Este
    // predicado es su **procedencia** y su canal de corrección: quién dijo que
    // algo era `vault`, con qué regla y cuándo. Cardinalidad `many` a propósito:
    // varias señales pueden apuntar al mismo archivo y todas son parte del
    // porqué. Ver `docs/adr/0006-donde-vive-la-sensibilidad.md`.
    ("sensitivity_level", "nivel de sensibilidad", "many", "inferred", None),
    // La marca de una **carpeta**: el suelo de sensibilidad de todo lo que
    // tenga dentro, también lo que llegue después. Sólo la escribe el usuario
    // (I5); ver `sensitivity.rs` y `docs/adr/0020-sensibilidad-por-carpeta.md`.
    ("contents_sensitivity", "sensibilidad de lo que contiene", "one", "user", None),
    // ── temporal · determinista ──
    ("occurred_during", "ocurrió durante", "many", "deterministic", None),
    ("precedes", "precede a", "many", "deterministic", None),
    ("supersedes", "reemplaza a", "many", "deterministic", None),
    // ── social · determinista ──
    ("authored_by", "escrito por", "many", "deterministic", None),
    ("sent_to", "enviado a", "many", "deterministic", None),
    ("mentions", "menciona", "many", "deterministic", None),
    // ── semántico · inferido ──
    ("about", "trata sobre", "many", "inferred", None),
    ("part_of_process", "parte del proceso", "many", "inferred", None),
    ("evidence_for", "prueba de", "many", "inferred", None),
    ("contradicts", "contradice a", "many", "inferred", None),
];
