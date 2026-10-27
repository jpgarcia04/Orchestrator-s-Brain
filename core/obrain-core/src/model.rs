//! Tipos del núcleo. Implementa el modelo de datos de `docs/PROJECT-MASTER.md` §3.
//!
//! La unidad primaria **no es el nodo: es la aserción**. Nodos y aristas son
//! proyecciones de un log append-only de aserciones inmutables.

use std::fmt;

// ─────────────────────────────────────────────────────────── identificadores

/// Identificador de entidad. ULID: ordenable por tiempo de creación.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(pub String);

impl EntityId {
    pub fn new() -> Self {
        EntityId(ulid::Ulid::new().to_string())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for EntityId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for EntityId {
    fn from(s: &str) -> Self {
        EntityId(s.to_string())
    }
}

impl From<String> for EntityId {
    fn from(s: String) -> Self {
        EntityId(s)
    }
}

/// Identificador de aserción. ULID: el orden lexicográfico es el orden de escritura.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AssertionId(pub String);

impl AssertionId {
    pub fn new() -> Self {
        AssertionId(ulid::Ulid::new().to_string())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for AssertionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for AssertionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ─────────────────────────────────────────────────────────────────── método

/// Cómo se obtuvo una aserción.
///
/// El valor discriminante **es** la precedencia: a mayor valor, mayor autoridad
/// al resolver un conflicto sobre el mismo `(sujeto, predicado)`.
///
/// `User` gana siempre — invariante I5. Y gana incluso a `Deterministic`,
/// que es la decisión menos obvia de este enum: un hash es objetivamente
/// correcto sobre hashes, pero el usuario puede estar corrigiendo la
/// *interpretación*, no el dato. Entre "el sistema tiene razón" y "el usuario
/// confía en el sistema", el producto necesita lo segundo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Method {
    /// **No certero y corregible.** Confianza < 1.0.
    ///
    /// Ampliado en M3: originalmente significaba "inferido por un modelo",
    /// pero al implementar los enlazadores deterministas apareció un tercer
    /// caso que no encajaba en ningún sitio — la **heurística mecánica**
    /// (`informe_v2.pdf` es versión de `informe_v1.pdf`). No interviene ningún
    /// modelo, pero tampoco es una certeza.
    ///
    /// En vez de añadir un cuarto método, `Inferred` pasa a significar
    /// "podría estar mal", y el campo `source` distingue quién lo produjo:
    /// `heuristic:filename-version` frente a `classifier@v1`. La precedencia
    /// es la misma en ambos casos, que es lo único que este enum decide.
    Inferred = 0,
    /// Importado de otro sistema. Fiable pero no verificado aquí.
    Imported = 1,
    /// Derivado mecánicamente: hash, EXIF, metadatos. Confianza 1.0.
    Deterministic = 2,
    /// Afirmado por el usuario. Precedencia absoluta y permanente.
    User = 3,
}

impl Method {
    pub fn precedence(self) -> u8 {
        self as u8
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Method::Inferred => "inferred",
            Method::Imported => "imported",
            Method::Deterministic => "deterministic",
            Method::User => "user",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "inferred" => Some(Method::Inferred),
            "imported" => Some(Method::Imported),
            "deterministic" => Some(Method::Deterministic),
            "user" => Some(Method::User),
            _ => None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────── valores

/// Objeto de una aserción: otra entidad, o un literal.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Entity(EntityId),
    Text(String),
    Number(f64),
    /// Epoch en milisegundos.
    Timestamp(i64),
    Bool(bool),
}

impl Value {
    pub fn kind(&self) -> &'static str {
        match self {
            Value::Entity(_) => "entity",
            Value::Text(_) => "text",
            Value::Number(_) => "number",
            Value::Timestamp(_) => "timestamp",
            Value::Bool(_) => "bool",
        }
    }

    pub fn as_entity(&self) -> Option<&EntityId> {
        match self {
            Value::Entity(e) => Some(e),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(t) => Some(t),
            _ => None,
        }
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::Text(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::Text(s)
    }
}

impl From<EntityId> for Value {
    fn from(e: EntityId) -> Self {
        Value::Entity(e)
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Value::Number(n)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

// ────────────────────────────────────────────────────────────── sensibilidad

/// Nivel de sensibilidad de una entidad. Gobierna la política de egreso
/// (`PROJECT-MASTER.md` §5.2, invariante I11).
///
/// Es la **única excepción** a "todo es una aserción": vive como columna en
/// `entities`. Un control de seguridad debe ser directamente consultable y no
/// depender de resolver una proyección con precedencias — un fallo de
/// resolución no puede traducirse en una fuga.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Sensitivity {
    Normal,
    Sensitive,
    Vault,
}

impl Sensitivity {
    pub fn as_str(self) -> &'static str {
        match self {
            Sensitivity::Normal => "normal",
            Sensitivity::Sensitive => "sensitive",
            Sensitivity::Vault => "vault",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "normal" => Some(Sensitivity::Normal),
            "sensitive" => Some(Sensitivity::Sensitive),
            "vault" => Some(Sensitivity::Vault),
            _ => None,
        }
    }

    /// ¿Puede el contenido de esta entidad salir de la máquina?
    pub fn may_egress(self) -> bool {
        matches!(self, Sensitivity::Normal)
    }
}

// ───────────────────────────────────────────────────────────────── facetas

/// Facetas conocidas (`PROJECT-MASTER.md` §3.2). El "tipo" de una entidad es
/// la composición de sus facetas, no una etiqueta rígida.
///
/// Se almacenan como aserciones con predicado `has_facet`, porque una faceta
/// puede ser **inferida** (un clasificador decide que un PDF es `Financial`)
/// y por tanto necesita confianza, procedencia y corrección como cualquier
/// otra afirmación.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Facet {
    Addressable,
    Textual,
    Temporal,
    Agent,
    Geolocated,
    Financial,
    Process,
    Confidential,
    /// Agrupa a otras entidades: una carpeta, una sesión de trabajo, un álbum.
    /// Añadida en M3 — ejemplo de que el modelo de facetas admite tipos nuevos
    /// sin migración, que era su razón de ser (§3.2).
    Container,
    /// Un proyecto de código: una carpeta que es un árbol de trabajo de git.
    ///
    /// No dice que su contenido no valga —lo escribió el usuario y se indexa
    /// entero—. Dice que **es una cosa, no doscientas**: un repositorio tiene
    /// dentro la estructura profunda y uniforme que [ADR-0015] describe, y sin
    /// esta faceta el cielo la confunde con la anchura de una vida.
    ///
    /// [ADR-0015]: ../../../docs/adr/0015-el-cielo-reparte-por-anchura-no-por-peso.md
    Project,
    /// Una carpeta que el usuario **eligió** indexar: la raíz de su bóveda.
    ///
    /// Lo que hay por encima de ella —`C:\Users\usuario\OneDrive`…— no es
    /// conocimiento del usuario sino el camino hasta él. Por eso el árbol de
    /// contención se corta aquí: el explorador empieza en la biblioteca y las
    /// migas no arrastran la ruta entera del disco.
    Library,
    /// Una **colección** del usuario: documentos que él junta a mano —«Trámite
    /// de la visa»— vengan de la carpeta que vengan, y un documento puede
    /// estar en varias. No tiene archivo ni ruta; sólo la crea el usuario (I5).
    ///
    /// Se llama `region` porque es la palabra del modelo (§10: *regiones del
    /// usuario*); en la interfaz es «colección». No es una región del cielo,
    /// que es una carpeta. Ver `docs/adr/0031-tus-colecciones.md`.
    Region,
}

impl Facet {
    pub fn as_str(self) -> &'static str {
        match self {
            Facet::Addressable => "addressable",
            Facet::Textual => "textual",
            Facet::Temporal => "temporal",
            Facet::Agent => "agent",
            Facet::Geolocated => "geolocated",
            Facet::Financial => "financial",
            Facet::Process => "process",
            Facet::Confidential => "confidential",
            Facet::Container => "container",
            Facet::Project => "project",
            Facet::Library => "library",
            Facet::Region => "region",
        }
    }
}

// ─────────────────────────────────────────────────────────────── predicados

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cardinality {
    /// Un solo valor vivo. Los conflictos se resuelven por precedencia.
    One,
    /// Varios valores vivos simultáneos son legítimos.
    Many,
}

impl Cardinality {
    pub fn as_str(self) -> &'static str {
        match self {
            Cardinality::One => "one",
            Cardinality::Many => "many",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "one" => Some(Cardinality::One),
            "many" => Some(Cardinality::Many),
            _ => None,
        }
    }
}

/// Definición de un predicado. Vocabulario **controlado**: las aristas con
/// texto libre hacen el grafo inconsultable (`PROJECT-MASTER.md` §3.4).
#[derive(Debug, Clone)]
pub struct PredicateDef {
    pub id: String,
    pub label: String,
    pub cardinality: Cardinality,
    pub symmetric: bool,
    pub transitive: bool,
    /// Predicado inverso. Si existe, la arista se almacena **una sola vez**
    /// y la inversa se deriva al consultar.
    pub inverse: Option<String>,
    pub default_method: Method,
}

impl PredicateDef {
    pub fn new(id: &str, label: &str, cardinality: Cardinality, default_method: Method) -> Self {
        PredicateDef {
            id: id.to_string(),
            label: label.to_string(),
            cardinality,
            symmetric: false,
            transitive: false,
            inverse: None,
            default_method,
        }
    }

    pub fn symmetric(mut self) -> Self {
        self.symmetric = true;
        self
    }

    pub fn transitive(mut self) -> Self {
        self.transitive = true;
        self
    }

    pub fn inverse(mut self, id: &str) -> Self {
        self.inverse = Some(id.to_string());
        self
    }
}

// ─────────────────────────────────────────────────────────────── aserciones

/// Una aserción escrita en el log. Inmutable salvo `retracted_at`.
#[derive(Debug, Clone)]
pub struct Assertion {
    pub id: AssertionId,
    pub subject: EntityId,
    pub predicate: String,
    pub object: Value,

    // ── bitemporalidad (invariante I6) ──
    /// Desde cuándo es cierto **en el mundo**. `None` = desde siempre.
    pub valid_from: Option<i64>,
    /// Hasta cuándo fue cierto en el mundo. `None` = sigue siéndolo.
    pub valid_to: Option<i64>,
    /// Cuándo lo supo el **sistema**.
    pub asserted_at: i64,
    /// Cuándo dejó el sistema de sostenerlo. `None` = vigente.
    /// Nunca se borra una fila: se retracta (invariante I4).
    pub retracted_at: Option<i64>,

    // ── procedencia (invariante I9) ──
    pub method: Method,
    pub source: String,
    pub confidence: f64,
    /// Dónde exactamente en el origen se apoya esto. Es lo que permite
    /// responder "¿por qué crees eso?".
    pub evidence: Option<String>,
}

impl Assertion {
    /// ¿Estaba vigente en `known`, y era cierta en `valid`?
    pub fn live_at(&self, at: TimePoint) -> bool {
        self.asserted_at <= at.known
            && self.retracted_at.is_none_or(|r| r > at.known)
            && self.valid_from.is_none_or(|v| v <= at.valid)
            && self.valid_to.is_none_or(|v| v > at.valid)
    }
}

/// Aserción a punto de escribirse.
#[derive(Debug, Clone)]
pub struct NewAssertion {
    pub subject: EntityId,
    pub predicate: String,
    pub object: Value,
    pub method: Method,
    pub source: String,
    pub confidence: f64,
    pub valid_from: Option<i64>,
    pub valid_to: Option<i64>,
    pub asserted_at: i64,
    pub evidence: Option<String>,
}

impl NewAssertion {
    /// Los métodos `User` y `Deterministic` fijan confianza 1.0: no tiene
    /// sentido que una afirmación del usuario o un hash sean "probables".
    pub fn new(
        subject: impl Into<EntityId>,
        predicate: &str,
        object: impl Into<Value>,
        method: Method,
        asserted_at: i64,
    ) -> Self {
        let confidence = match method {
            Method::User | Method::Deterministic => 1.0,
            _ => 0.5,
        };
        NewAssertion {
            subject: subject.into(),
            predicate: predicate.to_string(),
            object: object.into(),
            method,
            source: String::new(),
            confidence,
            valid_from: None,
            valid_to: None,
            asserted_at,
            evidence: None,
        }
    }

    pub fn source(mut self, s: &str) -> Self {
        self.source = s.to_string();
        self
    }

    pub fn confidence(mut self, c: f64) -> Self {
        self.confidence = c.clamp(0.0, 1.0);
        self
    }

    /// Intervalo de validez **en el mundo**, distinto de cuándo se supo.
    pub fn valid(mut self, from: Option<i64>, to: Option<i64>) -> Self {
        self.valid_from = from;
        self.valid_to = to;
        self
    }

    pub fn evidence(mut self, e: &str) -> Self {
        self.evidence = Some(e.to_string());
        self
    }
}

// ──────────────────────────────────────────────────────────── punto temporal

/// Punto de consulta bitemporal.
///
/// Los dos ejes responden preguntas distintas y ambas hacen falta:
/// - `valid`  — *"¿qué era cierto en el mundo en este momento?"*
/// - `known`  — *"…según lo que el sistema sabía en este otro momento?"*
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimePoint {
    pub valid: i64,
    pub known: i64,
}

impl TimePoint {
    pub fn new(valid: i64, known: i64) -> Self {
        TimePoint { valid, known }
    }

    /// El presente según el conocimiento actual: ambos ejes en `t`.
    pub fn at(t: i64) -> Self {
        TimePoint { valid: t, known: t }
    }

    /// *"¿Qué creía el sistema en `known` sobre cómo era el mundo en `valid`?"*
    /// Reconstruye una creencia pasada, incluyendo lo que luego se retractó.
    pub fn as_known_at(valid: i64, known: i64) -> Self {
        TimePoint { valid, known }
    }
}
