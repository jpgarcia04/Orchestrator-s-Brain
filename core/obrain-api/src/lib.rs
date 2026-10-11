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

pub mod colecciones;
pub mod dentro;
pub mod entity;
pub mod indexar;
pub mod lens;
pub mod marks;
pub mod papelera;
pub mod preview;
pub mod seguridad;
pub mod sky;
pub mod vigilar;

use obrain_core::{EntityId, Method, NewAssertion, Sensitivity, Store, TimePoint, Value};
/// El modelo, para quien lo carga y lo presta sin depender de `obrain-embed`.
pub use obrain_embed::embedder::Embedder;
use std::path::Path;
use std::time::Instant;

pub use colecciones::{Abierta, Coleccion, Miembro, Movidos, Pertenece};
pub use dentro::{Copia, Paquete, Parte};
pub use entity::{Claim, EntityView};
pub use indexar::IndexReport;
pub use lens::{Edge, Lens, Node};
pub use marks::{ByPart, MarkedFolder, Protection};
pub use papelera::EnPapelera;
pub use preview::Preview;
pub use sky::{Bridge, Cluster, Crumb, Expiring, Listing, Region, Row, Sky};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Core(#[from] obrain_core::Error),

    #[error(transparent)]
    Embed(#[from] obrain_embed::Error),

    #[error("no hay ninguna entidad con identificador {0}")]
    UnknownEntity(String),

    #[error(transparent)]
    Seguridad(#[from] seguridad::Error),

    #[error("está protegido con tu PIN: desbloquéalo para verlo o abrirlo")]
    Bloqueado,

    #[error("«{0}» no es una fecha: tiene que ser AAAA-MM-DD, y existir")]
    FechaInvalida(String),

    #[error(transparent)]
    Coleccion(#[from] colecciones::Error),

    #[error("el predicado «{0}» no admite correcciones")]
    CorreccionProhibida(String),

    #[error("tipo o valor incorrecto para «{0}»")]
    CorreccionInvalida(String),
}

impl Error {
    /// ¿Es «este equipo no puede abrir la llave de la base»? Es el único error
    /// de apertura con salida: la clave de recuperación ([`seguridad::recuperar`]).
    pub fn is_key_unavailable(&self) -> bool {
        use obrain_keys::Error::SistemaNoDisponible;
        matches!(
            self,
            Error::Core(obrain_core::Error::Key(SistemaNoDisponible(_)))
                | Error::Seguridad(seguridad::Error::Keys(SistemaNoDisponible(_)))
                | Error::Seguridad(seguridad::Error::Core(obrain_core::Error::Key(
                    SistemaNoDisponible(_)
                )))
        )
    }
}

/// Cuánto dura un desbloqueo con PIN desde que se autentica (ADR-0034).
pub const DESBLOQUEO: std::time::Duration = std::time::Duration::from_secs(5 * 60);

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
    /// El paquete de la lista bajo el que va este resultado: está dentro de
    /// él (ADR-0030). La lista ya viene ordenada así; la fila no se esconde.
    pub inside: Option<String>,
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
/// no, en Windows `%USERPROFILE%\.obrain\brain.obrain` —fuera de `AppData`,
/// donde Windows le da a cada app empaquetada su propia copia (ADR-0029)—, y
/// la primera vez muda ahí la base de `%LOCALAPPDATA%\obrain`. En el resto, el
/// directorio de datos del usuario. Ver [`obrain_core::ubicacion`].
pub fn default_db_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("OBRAIN_DB") {
        return std::path::PathBuf::from(p);
    }
    obrain_core::ubicacion::carpeta().join(obrain_core::ubicacion::BASE)
}

/// Qué reconocedor de texto (OCR) tiene este sistema, para decirlo: en
/// Windows, `windows.media.ocr/<idioma>`; fuera, ninguno.
pub fn ocr_engine() -> Option<String> {
    obrain_ocr::platform().map(|o| o.engine())
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

/// Lo que pasó al marcar o desmarcar una carpeta. Ver [`Brain::mark_folder`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct FolderMarked {
    /// La marca que queda puesta, o `None` si se quitó.
    pub level: Option<&'static str>,
    /// Entidades dentro de la carpeta —ella incluida— recalculadas.
    pub under: usize,
    /// De ellas, las que han cambiado de nivel.
    pub changed: usize,
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
    /// Con PIN: momento de autenticación. Las lecturas nunca lo renuevan.
    desbloqueo: std::cell::Cell<Option<Instant>>,
    clock: fn() -> Instant,
}

impl Brain {
    /// Abre la base **sin cargar el modelo**.
    ///
    /// Es deliberado, y es lo que hace posible la promesa de latencia de
    /// `EXPERIENCE.md` §2. La barra tiene que responder a la primera pulsación,
    /// y BM25 ya responde en 0,5–1,9 ms sin ningún modelo cargado. Cargarlo por
    /// adelantado convertiría 2,5 s de arranque en 2,5 s de espera delante del
    /// usuario, para una rama que quizá no necesite en esa consulta.
    ///
    /// La base **siempre queda cifrada** ([ADR-0021]): si no existe se crea
    /// cifrada, y si estaba en claro se cifra aquí. Si este equipo no puede
    /// abrir su llave, el error lo dice ([`Error::is_key_unavailable`]) y la
    /// salida es la clave de recuperación.
    ///
    /// [ADR-0021]: ../../../docs/adr/0021-cifrado-en-reposo-y-pin.md
    pub fn open(db: impl AsRef<Path>) -> Result<Self> {
        let store = seguridad::abrir(db.as_ref())?;
        obrain_extract::index::migrate(store.conn())?;
        obrain_embed::index::migrate(store.conn())?;
        Ok(Self::from_store(store))
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
        // Las tablas de las proyecciones y la de seguridad, por si la base es
        // nueva. Son `IF NOT EXISTS`: en una base ya hecha no cuestan nada.
        let _ = obrain_extract::index::migrate(store.conn());
        let _ = obrain_embed::index::migrate(store.conn());
        let _ = seguridad::migrate(store.conn());
        Brain {
            store,
            embedder: None,
            desbloqueo: std::cell::Cell::new(None),
            clock: Instant::now,
        }
    }

    /// Para los tests y para cualquier otro `Embedder` de §5.1.
    pub fn with_embedder(store: Store, embedder: Box<dyn Embedder + Send>) -> Self {
        let mut b = Self::from_store(store);
        b.embedder = Some(embedder);
        b
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
            inside: None,
        }
    }

    /// Busca. Híbrida si el modelo está cargado, léxica si todavía no.
    ///
    /// La degradación es la característica, no el plan B: una barra que no
    /// responde hasta que un modelo de 118 M de parámetros termine de cargarse
    /// ha perdido el argumento de §2 antes de enseñar nada.
    pub fn search(&self, query: &str, limit: usize) -> Result<Answer> {
        self.search_in(query, limit, None)
    }

    /// [`Brain::search`] **dentro de una colección** (ADR-0031), o en todo con
    /// `None`. El filtro se aplica en las dos ramas antes de cortar la lista:
    /// lo de la colección que estaba en el puesto 60 sale igual.
    pub fn search_in(
        &self,
        query: &str,
        limit: usize,
        coleccion: Option<&EntityId>,
    ) -> Result<Answer> {
        let t0 = Instant::now();
        let (_, at) = Self::now();
        let dentro = match coleccion {
            Some(c) => {
                if !colecciones::es(&self.store, c, at)? {
                    return Err(colecciones::Error::NoExiste.into());
                }
                Some(colecciones::en_la_boveda(&self.store, c, at)?)
            }
            None => None,
        };
        let dentro = dentro.as_ref();

        let (hits, branches, semantic): (Vec<Hit>, _, _) = match &self.embedder {
            Some(e) => {
                let (fused, _, _) = obrain_embed::search::hybrid_within(
                    &self.store,
                    e.as_ref(),
                    query,
                    limit,
                    at,
                    dentro,
                )?;
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
                let lexical = obrain_extract::search_within(&self.store, query, limit, at, dentro)?;
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

        // Con el PIN puesto y sin desbloquear, lo sensible sale en la lista
        // —su nombre ya está en el explorador— pero sin su fragmento de texto.
        let mut hits = hits;
        if !self.contenido_visible()? {
            for h in hits.iter_mut().filter(|h| h.sensitivity != "normal") {
                h.snippet.clear();
            }
        }

        // Lo de dentro, justo debajo de su paquete si el paquete también
        // salió: cinco filas que son tres documentos se leen como tres
        // (ADR-0030 §3). Ninguna se esconde.
        let mut paquetes: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for h in &hits {
            let de = self.store.query(
                Some(&EntityId(h.entity.clone())),
                Some(obrain_core::PART_OF),
                at,
            )?;
            let lista: Vec<String> = de
                .into_iter()
                .filter_map(|a| a.object.as_entity().map(|e| e.0.clone()))
                .collect();
            if !lista.is_empty() {
                paquetes.insert(h.entity.clone(), lista);
            }
        }
        if !paquetes.is_empty() {
            let ids: Vec<String> = hits.iter().map(|h| h.entity.clone()).collect();
            let mut viejos: Vec<Option<Hit>> = hits.into_iter().map(Some).collect();
            hits = dentro::agrupar(&ids, &paquetes)
                .into_iter()
                .filter_map(|(i, paquete)| {
                    viejos[i].take().map(|h| Hit {
                        inside: paquete,
                        ..h
                    })
                })
                .collect();
        }

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

    /// Los archivos más recientes. Ver [`sky::recent`].
    pub fn recent(&self, limit: usize) -> Result<Vec<Row>> {
        let (_, at) = Self::now();
        Ok(sky::recent(&self.store, limit, at)?)
    }

    /// **Lo que caduca pronto**: la tarjeta del inicio. Ver
    /// [`sky::expiring`] y [ADR-0023].
    ///
    /// Con el PIN puesto y bloqueado, lo sensible sale con su nombre y su
    /// carpeta y **sin la fecha**: la fecha sale del texto del documento, y
    /// el texto de lo sensible no sale del núcleo hasta desbloquear.
    ///
    /// [ADR-0023]: ../../../docs/adr/0023-vencimientos.md
    pub fn expiring(&self) -> Result<Vec<Expiring>> {
        let (now, at) = Self::now();
        let ocultar = !self.contenido_visible()?;
        Ok(sky::expiring(
            &self.store,
            at,
            obrain_expiry::Fecha::hoy(now),
            obrain_expiry::VENTANA_DIAS,
            ocultar,
        )?)
    }

    /// **Corrige cuándo caduca un documento**: `Some("2027-08-31")`, o `None`
    /// para «no caduca». Una aserción del usuario, que gana para siempre (I5)
    /// y ningún reindexado pisa.
    ///
    /// Con el PIN bloqueado, lo sensible no se corrige: para corregir una
    /// fecha hay que poder verla.
    pub fn set_expiry(&self, id: &EntityId, fecha: Option<&str>) -> Result<String> {
        let sensible = self
            .store
            .sensitivity(id)
            .map_err(|_| Error::UnknownEntity(id.0.clone()))?
            != Sensitivity::Normal;
        if sensible && !self.contenido_visible()? {
            return Err(Error::Bloqueado);
        }
        let fecha = match fecha {
            Some(t) => Some(
                obrain_expiry::Fecha::parse_iso(t.trim())
                    .ok_or_else(|| Error::FechaInvalida(t.to_string()))?,
            ),
            None => None,
        };
        let (now, _) = Self::now();
        let id = self
            .store
            .atomic_write(|s| obrain_expiry::set_by_user(s, id, fecha, now))?;
        Ok(id.0)
    }

    /// **La papelera**: lo que ya no está en tu bóveda y todavía no se ha
    /// olvidado. Ver [`papelera`] y [ADR-0025].
    ///
    /// [ADR-0025]: ../../../docs/adr/0025-lo-que-ya-no-esta.md
    pub fn trash(&self) -> Result<Vec<EnPapelera>> {
        let (_, at) = Self::now();
        Ok(papelera::list(&self.store, at)?)
    }

    /// **Olvida** lo que se leyó de un documento de la papelera, o de todos
    /// con `None` («Vaciar papelera»). No toca tus archivos: borra el texto y
    /// los vectores que obrain guardaba de ellos. El log se queda (I4).
    pub fn forget(&mut self, id: Option<&EntityId>) -> Result<usize> {
        if !self.contenido_visible()? {
            return Err(Error::Bloqueado);
        }
        let (now, _) = Self::now();
        self.store
            .transaction(|s| papelera::forget(s, id, Method::User, now))
    }

    /// Lo que la puerta retiene. Ver [`sky::sensitive`].
    pub fn sensitive(&self) -> Result<Vec<Row>> {
        let (_, at) = Self::now();
        Ok(sky::sensitive(&self.store, at)?)
    }

    /// La carpeta con esta ruta, si el log la conoce. Ver [`marks::folder_at`].
    pub fn folder_at(&self, path: &str) -> Result<Option<EntityId>> {
        let (_, at) = Self::now();
        Ok(marks::folder_at(&self.store, path, at)?)
    }

    /// Las carpetas que marcaste como sensibles. Ver [`marks::marked`].
    pub fn marked_folders(&self) -> Result<Vec<MarkedFolder>> {
        let (_, at) = Self::now();
        Ok(marks::marked(&self.store, at)?)
    }

    /// **Marca una carpeta como sensible** —todo lo que tenga dentro lo hereda,
    /// también lo que llegue después— o, con `None`, le quita la marca.
    ///
    /// Es una aserción del usuario (I5): ningún reescaneo la pisa. Va en una
    /// transacción porque son varias escrituras —la marca y la columna de todo
    /// lo que cuelga de la carpeta— y la puerta no puede leer un estado a
    /// medias. Ver [`obrain_guard::mark_folder`] y [ADR-0020].
    ///
    /// [ADR-0020]: ../../../docs/adr/0020-sensibilidad-por-carpeta.md
    pub fn mark_folder(
        &mut self,
        folder: &EntityId,
        level: Option<Sensitivity>,
    ) -> Result<FolderMarked> {
        if !self.contenido_visible()? {
            return Err(Error::Bloqueado);
        }
        let (now, _) = Self::now();
        let r = self
            .store
            .transaction(|s| obrain_guard::mark_folder(s, folder, level, now))?;
        Ok(FolderMarked {
            level: r.level.map(Sensitivity::as_str),
            under: r.under,
            changed: r.changed,
        })
    }

    // ───────────────────────────────────────────── tus colecciones (0031)

    /// **Tus colecciones**, por nombre. Ver [`colecciones`] y [ADR-0031].
    ///
    /// [ADR-0031]: ../../../docs/adr/0031-tus-colecciones.md
    pub fn collections(&self) -> Result<Vec<Coleccion>> {
        let (_, at) = Self::now();
        colecciones::todas(&self.store, at)
    }

    /// **Abre una colección**: lo que tiene, cada uno con su carpeta.
    pub fn collection(&self, id: &EntityId) -> Result<Abierta> {
        let (_, at) = Self::now();
        colecciones::abrir(&self.store, id, at)
    }

    /// El documento con esta ruta, si está en tu bóveda. Para la terminal.
    pub fn document_at(&self, ruta: &str) -> Result<Option<EntityId>> {
        let (_, at) = Self::now();
        Ok(colecciones::documento_en(&self.store, ruta, at)?)
    }

    /// La colección que se llama así, sin distinguir mayúsculas.
    pub fn collection_named(&self, nombre: &str) -> Result<Option<EntityId>> {
        let (_, at) = Self::now();
        Ok(colecciones::por_nombre(&self.store, nombre, at)?)
    }

    /// **Crea una colección** vacía. Devuelve su identificador. `source` dice
    /// desde dónde: `user@ui`, `user@cli`.
    pub fn create_collection(&mut self, nombre: &str, source: &str) -> Result<String> {
        let (now, _) = Self::now();
        let id = self
            .store
            .transaction(|s| colecciones::crear(s, nombre, now, source))?;
        Ok(id.0)
    }

    /// **Le cambia el nombre** a una colección.
    pub fn rename_collection(&mut self, id: &EntityId, nombre: &str, source: &str) -> Result<()> {
        let (now, _) = Self::now();
        self.store
            .transaction(|s| colecciones::renombrar(s, id, nombre, now, source))
    }

    /// **Borra una colección.** Tus documentos no se tocan. Devuelve cuántos
    /// tenía.
    pub fn delete_collection(&mut self, id: &EntityId) -> Result<usize> {
        if !self.contenido_visible()? {
            return Err(Error::Bloqueado);
        }
        let (now, _) = Self::now();
        self.store.transaction(|s| colecciones::borrar(s, id, now))
    }

    /// **Mete documentos** en una colección; devuelve cuántos entraron.
    pub fn add_to_collection(
        &mut self,
        id: &EntityId,
        docs: &[EntityId],
        source: &str,
    ) -> Result<usize> {
        for doc in docs {
            self.authorize_document(doc)?;
        }
        let (now, _) = Self::now();
        self.store
            .transaction(|s| colecciones::meter(s, id, docs, now, source))
    }

    /// **Saca documentos** de una colección; devuelve cuántos salieron.
    pub fn remove_from_collection(&mut self, id: &EntityId, docs: &[EntityId]) -> Result<usize> {
        for doc in docs {
            self.authorize_document(doc)?;
        }
        let (now, _) = Self::now();
        self.store
            .transaction(|s| colecciones::sacar(s, id, docs, now))
    }

    /// **Mueve documentos** de una colección a otra, de una vez: entran en
    /// `hacia` y salen de `desde`, o no cambia nada (ADR-0033).
    pub fn move_between_collections(
        &mut self,
        desde: &EntityId,
        hacia: &EntityId,
        docs: &[EntityId],
        source: &str,
    ) -> Result<Movidos> {
        for doc in docs {
            self.authorize_document(doc)?;
        }
        let (now, _) = Self::now();
        self.store
            .transaction(|s| colecciones::mover(s, desde, hacia, docs, now, source))
    }

    /// Los documentos que llevan otros dentro, con lo que llevan (ADR-0030).
    /// Es `obrain paquetes`.
    pub fn packages(&self) -> Result<Vec<Paquete>> {
        let (_, at) = Self::now();
        let mut packages = dentro::packages(&self.store, at)?;
        if !self.contenido_visible()? {
            for p in &mut packages {
                if p.sensitivity != "normal" {
                    p.parts.clear();
                }
            }
        }
        Ok(packages)
    }

    /// Abre la lente sobre una entidad. Ver [`lens`].
    pub fn lens(&self, focus: &[EntityId], radius: u8, budget: usize) -> Result<Lens> {
        let (_, at) = Self::now();
        let mut lens = lens::open(&self.store, focus, radius, budget, at)?;
        if !self.contenido_visible()? {
            for edge in &mut lens.edges {
                edge.source.clear();
            }
        }
        Ok(lens)
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
        self.glance_in(query, limit, None)
    }

    /// [`Brain::glance`] dentro de una colección, o en todo con `None`.
    pub fn glance_in(
        &self,
        query: &str,
        limit: usize,
        coleccion: Option<&EntityId>,
    ) -> Result<Glance> {
        let answer = self.search_in(query, limit, coleccion)?;
        let focus: Vec<EntityId> = answer
            .hits
            .iter()
            .map(|h| EntityId(h.entity.clone()))
            .collect();
        let lens = self.lens(&focus, 1, lens::SUBSTRATE_BUDGET)?;
        Ok(Glance { answer, lens })
    }

    /// Todo lo que se sabe de una entidad. Ver [`entity`].
    pub fn entity(&self, id: &EntityId) -> Result<EntityView> {
        let (_, at) = Self::now();
        // Que exista se comprueba aquí y no dentro: una entidad desconocida es
        // un error del llamante, no una vista vacía que parezca un archivo sin
        // datos.
        let nivel = self
            .store
            .sensitivity(id)
            .map_err(|_| Error::UnknownEntity(id.0.clone()))?;
        let mut v = entity::view(&self.store, id, at)?;
        // Las correcciones históricas tampoco pueden decidir qué archivo
        // abre la aplicación: la ruta procede de una observación determinista.
        v.path = self
            .store
            .query(Some(id), Some("path_observed_at"), at)?
            .into_iter()
            .find(|a| a.method == Method::Deterministic)
            .and_then(|a| a.object.as_text().map(str::to_string));
        let locked = nivel != Sensitivity::Normal && !self.contenido_visible()?;
        // Lista pública explícita; cualquier literal nuevo queda protegido.
        // La procedencia/evidencia libre puede contener contenido documental.
        for claims in [&mut v.observed, &mut v.inferred] {
            claims.retain(|c| !locked || c.public);
            for c in claims {
                if c.predicate == "downloaded_from" {
                    c.value = obrain_link::provenance::safe_origin(&c.value).unwrap_or_default();
                    c.evidence = None;
                    c.source = "linker:zone-identifier".into();
                }
                let target_locked = if let Some(target) = &c.value_entity {
                    self.store.sensitivity(&EntityId(target.clone()))? != Sensitivity::Normal
                        && !self.contenido_visible()?
                } else {
                    false
                };
                if locked || target_locked {
                    c.evidence = None;
                    c.source.clear();
                    c.span = None;
                    c.target_span = None;
                }
            }
        }
        if locked {
            v.parts.clear();
        }
        // Un documento caduca una vez: se enseña **la fecha que manda** —la
        // tuya si la corregiste (I5)— y no también la inferencia corregida,
        // que sigue en el log pero ya no es lo que el sistema sostiene
        // (EXPERIENCE §4.5: de pálida a sólida). Y la fecha sale del texto del
        // documento: con el PIN bloqueado, la de lo sensible no sale del
        // núcleo (ADR-0023).
        let manda = if locked {
            None
        } else {
            self.store
                .resolve(id, obrain_expiry::PREDICATE, at)?
                .map(|a| a.id.0)
        };
        let vale = |c: &Claim| {
            c.predicate != obrain_expiry::PREDICATE || Some(&c.assertion) == manda.as_ref()
        };
        v.observed.retain(vale);
        v.inferred.retain(vale);
        // Lo que está en la papelera no es «Relacionado» de nadie (ADR-0025).
        let fuera = obrain_scan::ausentes::missing_set(&self.store, at)?;
        let vivo = |c: &Claim| {
            c.value_entity
                .as_ref()
                .is_none_or(|e| !fuera.contains(&EntityId(e.clone())))
        };
        v.observed.retain(vivo);
        v.inferred.retain(vivo);
        Ok(v)
    }

    /// **Qué se ve del archivo.** Ver [`preview`].
    ///
    /// `query` sólo decide **dónde se abre** el texto —el tramo que explica la
    /// consulta, igual que la prueba de la lista de resultados— y nunca cuánto
    /// se puede leer. Lo que se puede leer lo decide la guarda de M1, y no la
    /// toca ningún argumento de esta función.
    pub fn preview(&self, id: &EntityId, query: Option<&str>) -> Result<Preview> {
        self.preview_at(id, query, None)
    }

    /// [`Brain::preview`] abierto **por un tramo** del texto, en bytes: dónde
    /// está un documento dentro de su paquete (ADR-0030). Manda sobre la
    /// consulta, y como ella sólo decide dónde se abre.
    pub fn preview_at(
        &self,
        id: &EntityId,
        query: Option<&str>,
        tramo: Option<(usize, usize)>,
    ) -> Result<Preview> {
        let (_, at) = Self::now();
        self.store
            .sensitivity(id)
            .map_err(|_| Error::UnknownEntity(id.0.clone()))?;
        let visible = self.contenido_visible()?;
        let p = preview::of(&self.store, id, query, tramo, at, !visible)?;
        Ok(p)
    }

    // ─────────────────────────────────────────────────────── seguridad (0021)

    fn ahora_ms() -> i64 {
        Self::now().0
    }

    fn tiene_pin(&self) -> Result<bool> {
        Ok(seguridad::tiene_pin(self.store.conn())?)
    }

    /// ¿Está desbloqueado? Caduca a los [`DESBLOQUEO`] desde autenticación.
    fn desbloqueada(&self) -> bool {
        match self.desbloqueo.get() {
            Some(t) if (self.clock)().saturating_duration_since(t) < DESBLOQUEO => true,
            Some(_) => {
                self.desbloqueo.set(None);
                false
            }
            None => false,
        }
    }

    fn tocar(&self) {
        self.desbloqueo.set(Some((self.clock)()));
    }

    /// ¿Puede salir del núcleo el contenido de lo sensible? Sin PIN, sí: se
    /// enseña borroso hasta un clic, como siempre. Con PIN, sólo desbloqueado.
    fn contenido_visible(&self) -> Result<bool> {
        Ok(!self.tiene_pin()? || self.desbloqueada())
    }

    /// ¿Se puede abrir este archivo con su programa? Lo sensible, con el PIN
    /// puesto, sólo desbloqueado: abrirlo fuera sería saltarse el PIN.
    pub fn may_open(&self, id: &EntityId) -> Result<bool> {
        let sensible = self
            .store
            .sensitivity(id)
            .map_err(|_| Error::UnknownEntity(id.0.clone()))?
            != Sensitivity::Normal;
        if !sensible || self.contenido_visible()? {
            return Ok(true);
        }
        Ok(false)
    }

    /// Autoriza abrir/revelar y resuelve sólo rutas observadas por el escáner.
    pub fn path_to_open(&self, id: &EntityId) -> Result<Option<String>> {
        if !self.may_open(id)? {
            return Err(Error::Bloqueado);
        }
        let (_, at) = Self::now();
        Ok(self
            .store
            .query(Some(id), Some("path_observed_at"), at)?
            .into_iter()
            .find(|a| a.method == Method::Deterministic)
            .and_then(|a| a.object.as_text().map(str::to_string)))
    }

    /// Cómo está la seguridad de la base, para enseñarlo.
    pub fn security(&self) -> Result<seguridad::Estado> {
        let mut e = seguridad::estado(self.store.conn(), Self::ahora_ms())?;
        e.desbloqueada = e.pin && self.desbloqueada();
        e.restante_ms = if e.desbloqueada {
            self.desbloqueo.get().map(|t| {
                DESBLOQUEO
                    .saturating_sub((self.clock)().saturating_duration_since(t))
                    .as_millis() as u64
            })
        } else {
            None
        };
        Ok(e)
    }

    /// Desbloquea con el PIN. Tras tres fallos, espera creciente.
    pub fn unlock(&self, pin: &str) -> Result<()> {
        seguridad::comprobar_pin(self.store.conn(), pin, Self::ahora_ms())?;
        self.tocar();
        Ok(())
    }

    /// Vuelve a bloquear ya, sin esperar a que caduque.
    pub fn lock(&self) {
        self.desbloqueo.set(None);
    }

    /// Pone o cambia el PIN. Si ya había uno, hay que saberlo.
    pub fn set_pin(&self, nuevo: &str, actual: Option<&str>) -> Result<()> {
        if self.tiene_pin()? {
            seguridad::comprobar_pin(self.store.conn(), actual.unwrap_or(""), Self::ahora_ms())?;
        }
        seguridad::poner_pin(self.store.conn(), nuevo)?;
        self.tocar();
        Ok(())
    }

    /// Quita el PIN. Hay que saberlo.
    pub fn remove_pin(&self, actual: &str) -> Result<()> {
        seguridad::comprobar_pin(self.store.conn(), actual, Self::ahora_ms())?;
        seguridad::quitar_pin(self.store.conn())?;
        self.lock();
        Ok(())
    }

    /// **Olvidé el PIN.** Con la clave de recuperación se pone uno nuevo (o
    /// ninguno, con `None`).
    pub fn reset_pin(&self, codigo: &str, nuevo: Option<&str>) -> Result<()> {
        seguridad::comprobar_recuperacion(self.store.conn(), codigo)?;
        match nuevo {
            Some(p) => seguridad::poner_pin(self.store.conn(), p)?,
            None => seguridad::quitar_pin(self.store.conn())?,
        }
        self.tocar();
        Ok(())
    }

    /// La clave de recuperación, para volver a verla. Con PIN, desbloqueado.
    pub fn recovery_code(&self) -> Result<Option<String>> {
        if !self.contenido_visible()? {
            return Err(Error::Bloqueado);
        }
        Ok(seguridad::clave_de_recuperacion(self.store.conn())?)
    }

    /// Ya la guardaste: la app deja de enseñarla al abrir.
    pub fn confirm_recovery_saved(&self) -> Result<()> {
        Ok(seguridad::confirmar_recuperacion(self.store.conn())?)
    }

    /// Corrige. Escribe una aserción del usuario, que gana para siempre (I5).
    ///
    /// No retracta la inferencia que corrige, y es intencionado: la resolución
    /// por precedencia de [`Store::resolve`] ya hace que la del usuario mande,
    /// y conservar la otra es lo que permite responder más tarde *qué creía el
    /// sistema y en qué se equivocó*. Borrar el error borraría la prueba de que
    /// la corrección sirvió de algo.
    ///
    /// Decir lo mismo otra vez no escribe nada ([`Store::assert_by_user`]):
    /// un «es correcto» pulsado tres veces amontonaba tres filas en la vista
    /// previa (JP, 2026-09-24).
    pub fn correct(&self, subject: &EntityId, predicate: &str, object: Value) -> Result<String> {
        if !entity::correctable(predicate) {
            return Err(Error::CorreccionProhibida(predicate.into()));
        }
        self.authorize_document(subject)?;
        match (predicate, &object) {
            ("expires_on", Value::Text(t)) if t == "nunca" => {}
            ("expires_on", Value::Text(t)) if obrain_expiry::Fecha::parse_iso(t).is_some() => {}
            ("sensitivity_level", Value::Text(t)) if Sensitivity::parse(t).is_some() => {}
            ("version_of" | "occurred_during", Value::Entity(target)) if target != subject => {
                self.authorize_document(target)?;
            }
            _ => return Err(Error::CorreccionInvalida(predicate.into())),
        }
        let (now, _) = Self::now();
        let id = self.store.atomic_write(|s| -> Result<_> {
            let id = s.assert_by_user(
                NewAssertion::new(subject.clone(), predicate, object, Method::User, now)
                    .source("user@ui"),
            )?;
            if predicate == "sensitivity_level" {
                s.reproject_sensitivity(subject, TimePoint::at(now))?;
            }
            Ok(id)
        })?;
        Ok(id.0)
    }

    fn authorize_document(&self, id: &EntityId) -> Result<()> {
        let level = self.store.sensitivity(id)?;
        if level != Sensitivity::Normal && !self.contenido_visible()? {
            return Err(Error::Bloqueado);
        }
        Ok(())
    }

    /// Confirma una aserción vigente recuperando su objeto tipado del log.
    pub fn confirm(&self, subject: &EntityId, assertion: &str) -> Result<String> {
        self.authorize_document(subject)?;
        let (_, at) = Self::now();
        let a = self
            .store
            .query(Some(subject), None, at)?
            .into_iter()
            .find(|a| a.id.as_str() == assertion)
            .ok_or_else(|| obrain_core::Error::UnknownAssertion(assertion.into()))?;
        self.correct(subject, &a.predicate, a.object)
    }
}

#[cfg(test)]
mod pin_clock_tests {
    use super::*;
    thread_local! {
        static CLOCK: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
    }
    fn clock() -> Instant {
        CLOCK.with(|c| c.get().unwrap())
    }

    #[test]
    fn caduca_sin_esperar_y_ninguna_lectura_renueva_el_pin() {
        let t = Instant::now();
        CLOCK.with(|c| c.set(Some(t)));
        let mut b = Brain::from_store(Store::open_in_memory().unwrap());
        b.clock = clock;
        let e = b.store.create_entity(1).unwrap();
        b.store
            .set_sensitivity(&e, Sensitivity::Sensitive, 1)
            .unwrap();
        b.set_pin("2468", None).unwrap();
        assert_eq!(b.security().unwrap().restante_ms, Some(300_000));
        CLOCK.with(|c| c.set(Some(t + DESBLOQUEO - std::time::Duration::from_millis(1))));
        assert!(b.may_open(&e).unwrap());
        b.preview(&e, None).unwrap();
        b.entity(&e).unwrap();
        b.search("sintetico", 10).unwrap();
        assert_eq!(b.security().unwrap().restante_ms, Some(1));
        CLOCK.with(|c| c.set(Some(t + DESBLOQUEO)));
        assert!(!b.security().unwrap().desbloqueada);
        assert_eq!(b.security().unwrap().restante_ms, None);
        assert!(!b.may_open(&e).unwrap());
        assert!(b.preview(&e, None).unwrap().locked);
        assert!(matches!(
            b.correct(&e, "expires_on", "2030-01-01".into()),
            Err(Error::Bloqueado)
        ));
        assert!(matches!(
            b.set_expiry(&e, Some("2030-01-01")),
            Err(Error::Bloqueado)
        ));
        assert!(matches!(b.recovery_code(), Err(Error::Bloqueado)));
    }
}
