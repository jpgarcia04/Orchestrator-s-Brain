-- Copia del DDL de core/obrain-core/src/schema.rs en 683e63b (v1).
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
