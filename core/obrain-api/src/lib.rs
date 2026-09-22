//! # obrain-api — M5 · La superficie del núcleo
//!
//! Cuatro operaciones: **buscar, abrir la lente, mirar una entidad y
//! corregirla**. Nada más, y a propósito.
//!
//! ## Por qué existe este crate y no vive dentro de la aplicación de escritorio
//!
//! Hasta M4 cada hito era una librería con su CLI encima, y funcionaba: el CLI
//! carga el modelo, hace una cosa y muere. Una interfaz no puede trabajar así,
//! y las tres diferencias no son de comodidad:
//!
//! 1. **Alguien tiene que ser dueño del estado caro.** Cargar
//!    `multilingual-e5-small` cuesta **2,50 s** (informe de M4). En un CLI da
//!    igual; en una barra que se abre cuarenta veces al día es la diferencia
//!    entre un producto y una demo. El modelo se carga una vez y vive aquí.
//! 2. **[`search::hybrid`](obrain_embed::search::hybrid) devuelve tres listas**
//!    —fusionada, BM25 y kNN— porque el informe de un hito tiene que poder
//!    enseñarlas lado a lado. Es la firma correcta para medir y la equivocada
//!    para dibujar.
//! 3. **Del grafo sólo había escritura.** `obrain-link` lleva desde M3
//!    produciendo aristas que nadie leía. La lente de I2 —foco, radio y
//!    presupuesto— no existía en ninguna parte; ahora está en [`lens`].
//!
//! Y el motivo por el que esto **no** es el backend de la aplicación: el
//! contestador de M7 necesita exactamente estas cuatro operaciones. Escribirlas
//! dentro de Tauri sería escribirlas dos veces, y la segunda vez con prisa.
//!
//! ## Lo que este crate deliberadamente NO hace
//!
//! **No mantiene la matriz de vectores en memoria.** [ADR-0010] midió que el
//! 91 % del coste del kNN es deserializar, y que la matriz residente subiría el
//! techo de ~19.700 vectores a ~219.000. Con 2.479 hoy, y una consulta híbrida
//! completa en 43 ms contra un presupuesto de 50, esa optimización resolvería
//! un problema que está a un factor 8 de distancia — a cambio de duplicar el
//! ensamblaje de M4. Se hace cuando el número lo pida, y el número está escrito.
//!
//! [ADR-0010]: ../../../docs/adr/0010-recorrido-lineal-frente-a-indice-ann.md

pub mod entity;
pub mod indexar;
pub mod lens;
pub mod preview;
pub mod sky;

use obrain_core::{EntityId, Method, NewAssertion, Store, TimePoint, Value};
/// El modelo, para quien lo carga y lo presta sin depender de `obrain-embed`.
pub use obrain_embed::embedder::Embedder;
use std::path::Path;
use std::time::Instant;

pub use entity::{Claim, EntityView};
pub use indexar::IndexReport;
pub use lens::{Edge, Lens, Node};
pub use preview::Preview;
pub use sky::{Bridge, Cluster, Crumb, Listing, Region, Row, Sky};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Core(#[from] obrain_core::Error),

    #[error(transparent)]
    Embed(#[from] obrain_embed::Error),

    #[error("no hay ninguna entidad con identificador {0}")]
    UnknownEntity(String),
}

/// Un resultado de búsqueda, ya con todo lo que hace falta para enseñarlo.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Hit {
    pub entity: String,
    pub label: String,
    pub path: String,
    pub snippet: String,
    pub score: f64,
    /// `(rama, puesto)`. La procedencia de I9 llevada a cada fila de la lista:
    /// se puede decir *por qué* este resultado está aquí, y no sólo que lo está.
    pub ranks: Vec<(String, usize)>,
    pub sensitivity: &'static str,
}

/// La respuesta a una consulta, con lo que se ha usado para producirla.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Answer {
    pub hits: Vec<Hit>,
    /// Los mismos resultados **agrupados por la carpeta que comparten**.
    ///
    /// No duplica nada: los grupos guardan identificadores, no copias. Y no es
    /// una comodidad de presentación — doce archivos en fila son casi siempre
    /// tres cosas, y enseñarlos planos obliga a la persona a reconstruir a mano
    /// una agrupación que el sistema ya tenía escrita desde M3.
    pub clusters: Vec<Cluster>,
    /// Qué ramas de §6 corrieron de verdad. Hoy `bm25` y, si el modelo está
    /// cargado, `knn`. Las otras dos de §6 necesitan resolución de entidad (M7).
    pub branches: Vec<&'static str>,
    /// El modelo estaba listo. Si es `false`, la respuesta es sólo léxica —
    /// correcta, pero incapaz de encontrar por significado.
    pub semantic: bool,
    pub elapsed_ms: f64,
}

/// Dónde vive la base **por defecto**: la misma para la aplicación y para
/// `obrain indexar`, de modo que lo que indexa uno lo ve la otra sin copiar
/// nada ni configurar una variable.
///
/// `OBRAIN_DB` manda si está puesta, para poder abrir cualquier otra base. Si
/// no, en Windows `%LOCALAPPDATA%\obrain\brain.obrain`; en el resto, el
/// directorio de datos del usuario.
pub fn default_db_path() -> std::path::PathBuf {
    use std::path::PathBuf;
    if let Ok(p) = std::env::var("OBRAIN_DB") {
        return PathBuf::from(p);
    }
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_DATA_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("obrain").join("brain.obrain")
}

/// Carga el modelo de embeddings, o `None` si todavía no está descargado.
///
/// Fuera de [`Brain`] a propósito: son **2,50 s** que hay que poder pagar en un
/// hilo de fondo, sin tener cogido el cerebro. Que el modelo no esté no es un
/// error del que haya que recuperarse — la mitad semántica simplemente no se
/// enciende y [`Answer::semantic`] lo dice.
pub fn load_model(model_dir: Option<&Path>) -> Result<Option<Box<dyn Embedder + Send>>> {
    let dir = model_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(obrain_embed::download::default_dir);
    if !obrain_embed::download::is_present(&dir) {
        return Ok(None);
    }
    Ok(Some(Box::new(obrain_embed::e5::E5::load(&dir)?)))
}

/// Una consulta y su sustrato, resueltos de una vez. Ver [`Brain::glance`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct Glance {
    pub answer: Answer,
    pub lens: Lens,
}

/// El sistema, abierto y listo para responder.
pub struct Brain {
    store: Store,
    /// `+ Send` porque el dueño real de esto es una aplicación con hilos: el
    /// modelo se carga en uno de fondo mientras la barra ya responde en otro.
    /// Es la restricción que hace posible que [`Brain::warm`] no sea una espera
    /// delante del usuario.
    embedder: Option<Box<dyn Embedder + Send>>,
}

impl Brain {
    /// Abre la base **sin cargar el modelo**.
    ///
    /// Es deliberado, y es lo que hace posible la promesa de latencia de
    /// `EXPERIENCE.md` §2. La barra tiene que responder a la primera pulsación,
    /// y BM25 ya responde en 0,5–1,9 ms sin ningún modelo cargado. Cargarlo por
    /// adelantado convertiría 2,5 s de arranque en 2,5 s de espera delante del
    /// usuario, para una rama que quizá no necesite en esa consulta.
    pub fn open(db: impl AsRef<Path>) -> Result<Self> {
        let store = Store::open(db)?;
        obrain_extract::index::migrate(store.conn())?;
        obrain_embed::index::migrate(store.conn())?;
        Ok(Brain { store, embedder: None })
    }

    /// Carga el modelo. Pensado para llamarse **en segundo plano** justo
    /// después de [`Brain::open`]: mientras tarda, la búsqueda ya funciona en
    /// modo léxico y [`Answer::semantic`] dice `false`.
    ///
    /// Si el modelo no está descargado no es un error del que haya que
    /// recuperarse: la mitad semántica simplemente no se enciende.
    pub fn warm(&mut self, model_dir: Option<&Path>) -> Result<bool> {
        match load_model(model_dir)? {
            Some(e) => {
                self.set_embedder(e);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Instala un modelo ya cargado. **Es instantáneo**, y por eso existe
    /// aparte de [`Brain::warm`]: quien tenga el cerebro tras un cerrojo puede
    /// pagar los 2,5 s de carga fuera de él y entrar sólo para dejar el modelo
    /// puesto. Con `warm` a secas, esos 2,5 s serían 2,5 s de barra bloqueada.
    pub fn set_embedder(&mut self, embedder: Box<dyn Embedder + Send>) {
        self.embedder = Some(embedder);
    }

    /// Presta el modelo a quien va a indexar, **sin tener cogido el cerebro**.
    ///
    /// Vectorizar documentos nuevos puede tardar; hacerlo con el cerrojo puesto
    /// dejaría la barra sin responder todo ese rato. Mientras está prestado la
    /// búsqueda sigue funcionando en léxico, y [`Answer::semantic`] lo dice.
    /// Se devuelve con [`Brain::set_embedder`].
    pub fn take_embedder(&mut self) -> Option<Box<dyn Embedder + Send>> {
        self.embedder.take()
    }

    /// Sobre una base ya abierta. El caso de una base en memoria en los tests,
    /// y el de quien ya tiene un `Store` montado.
    pub fn from_store(store: Store) -> Self {
        Brain { store, embedder: None }
    }

    /// Para los tests y para cualquier otro `Embedder` de §5.1.
    pub fn with_embedder(store: Store, embedder: Box<dyn Embedder + Send>) -> Self {
        Brain { store, embedder: Some(embedder) }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn semantic_ready(&self) -> bool {
        self.embedder.is_some()
    }

    fn now() -> (i64, TimePoint) {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        (ms, TimePoint::at(ms))
    }

    fn decorate(&self, entity: &EntityId, path: String, at: TimePoint) -> Hit {
        Hit {
            label: obrain_link::label(&self.store, entity, at),
            sensitivity: self
                .store
                .sensitivity(entity)
                .map(|s| s.as_str())
                .unwrap_or("vault"),
            entity: entity.0.clone(),
            path,
            snippet: String::new(),
            score: 0.0,
            ranks: Vec::new(),
        }
    }

    /// Busca. Híbrida si el modelo está cargado, léxica si todavía no.
    ///
    /// La degradación es la característica, no el plan B: una barra que no
    /// responde hasta que un modelo de 118 M de parámetros termine de cargarse
    /// ha perdido el argumento de §2 antes de enseñar nada.
    pub fn search(&self, query: &str, limit: usize) -> Result<Answer> {
        let t0 = Instant::now();
        let (_, at) = Self::now();

        let (hits, branches, semantic): (Vec<Hit>, _, _) = match &self.embedder {
            Some(e) => {
                let (fused, _, _) =
                    obrain_embed::search::hybrid(&self.store, e.as_ref(), query, limit, at)?;
                let hits = fused
                    .into_iter()
                    .map(|h| Hit {
                        snippet: h.snippet,
                        score: h.score,
                        ranks: h.ranks,
                        ..self.decorate(&h.entity, h.path, at)
                    })
                    .collect();
                (hits, vec!["bm25", "knn"], true)
            }
            None => {
                let lexical = obrain_extract::search(&self.store, query, limit, at)?;
                let hits = lexical
                    .into_iter()
                    .enumerate()
                    .map(|(i, h)| Hit {
                        snippet: h.snippet,
                        score: h.score,
                        ranks: vec![("bm25".to_string(), i + 1)],
                        ..self.decorate(&h.entity, h.path, at)
                    })
                    .collect();
                (hits, vec!["bm25"], false)
            }
        };

        Ok(Answer {
            clusters: sky::cluster(&self.store, &hits, at)?,
            hits,
            branches,
            semantic,
            elapsed_ms: t0.elapsed().as_secs_f64() * 1000.0,
        })
    }

    /// El cielo: el grafo entero a poca escala. Ver [`sky`].
    ///
    /// No se cachea, y a propósito: son dos consultas al log y el coste sube con
    /// el número de carpetas, no con el de archivos. El día que deje de ser
    /// instantáneo, cachear será una decisión con un número detrás.
    pub fn sky(&self, target: usize) -> Result<Sky> {
        let (_, at) = Self::now();
        Ok(sky::regions(&self.store, target, at)?)
    }

    /// **Qué hay dentro de esto.** El explorador de [ADR-0014]. Ver
    /// [`sky::children`].
    ///
    /// Con `focus` a `None` devuelve las raíces. Es la misma lectura del árbol
    /// que [`Brain::sky`], un escalón más abajo — y es la misma a propósito: dos
    /// vistas del mismo estado no pueden salir de dos cálculos distintos.
    ///
    /// [ADR-0014]: ../../../docs/adr/0014-dos-vistas-un-solo-estado.md
    pub fn children(&self, focus: Option<&EntityId>) -> Result<Listing> {
        let (_, at) = Self::now();
        Ok(sky::children(&self.store, focus, at)?)
    }

    /// Abre la lente sobre una entidad. Ver [`lens`].
    pub fn lens(&self, focus: &[EntityId], radius: u8, budget: usize) -> Result<Lens> {
        let (_, at) = Self::now();
        Ok(lens::open(&self.store, focus, radius, budget, at)?)
    }

    /// Una consulta y **lo que enciende**: la lista de resultados y el sustrato
    /// que `ADR-0004` pide detrás de la barra, en una sola llamada.
    ///
    /// Que sea una sola llamada no es comodidad de transporte. Pedir la lista y
    /// el sustrato por separado ejecutaría la búsqueda **dos veces por
    /// pulsación**, y una barra que se escribe a 60 palabras por minuto no
    /// puede permitirse hacer el doble del trabajo para enseñar lo mismo.
    ///
    /// Radio 1 y no 2 a propósito. Con doce focos a la vez, el radio 2 hace
    /// estallar el vecindario y el presupuesto acaba cortando por donde no
    /// debe; lo que tiene que verse de reojo es *dónde* cae la respuesta, no el
    /// grafo entero alrededor. Y con [`lens::SUBSTRATE_BUDGET`], que es más
    /// bajo que el del modo Foco por el mismo motivo — y porque el layout del
    /// cliente es cuadrático y esto se redibuja en cada pulsación.
    pub fn glance(&self, query: &str, limit: usize) -> Result<Glance> {
        let answer = self.search(query, limit)?;
        let focus: Vec<EntityId> =
            answer.hits.iter().map(|h| EntityId(h.entity.clone())).collect();
        let lens = self.lens(&focus, 1, lens::SUBSTRATE_BUDGET)?;
        Ok(Glance { answer, lens })
    }

    /// Todo lo que se sabe de una entidad. Ver [`entity`].
    pub fn entity(&self, id: &EntityId) -> Result<EntityView> {
        let (_, at) = Self::now();
        // Que exista se comprueba aquí y no dentro: una entidad desconocida es
        // un error del llamante, no una vista vacía que parezca un archivo sin
        // datos.
        self.store
            .sensitivity(id)
            .map_err(|_| Error::UnknownEntity(id.0.clone()))?;
        Ok(entity::view(&self.store, id, at)?)
    }

    /// **Qué se ve del archivo.** Ver [`preview`].
    ///
    /// `query` sólo decide **dónde se abre** el texto —el tramo que explica la
    /// consulta, igual que la prueba de la lista de resultados— y nunca cuánto
    /// se puede leer. Lo que se puede leer lo decide la guarda de M1, y no la
    /// toca ningún argumento de esta función.
    pub fn preview(&self, id: &EntityId, query: Option<&str>) -> Result<Preview> {
        let (_, at) = Self::now();
        self.store
            .sensitivity(id)
            .map_err(|_| Error::UnknownEntity(id.0.clone()))?;
        Ok(preview::of(&self.store, id, query, at)?)
    }

    /// Corrige. Escribe una aserción del usuario, que gana para siempre (I5).
    ///
    /// No retracta la inferencia que corrige, y es intencionado: la resolución
    /// por precedencia de [`Store::resolve`] ya hace que la del usuario mande,
    /// y conservar la otra es lo que permite responder más tarde *qué creía el
    /// sistema y en qué se equivocó*. Borrar el error borraría la prueba de que
    /// la corrección sirvió de algo.
    pub fn correct(
        &self,
        subject: &EntityId,
        predicate: &str,
        object: Value,
    ) -> Result<String> {
        let (now, _) = Self::now();
        let id = self.store.assert(
            NewAssertion::new(subject.clone(), predicate, object, Method::User, now)
                .source("user@ui"),
        )?;
        Ok(id.0)
    }
}
