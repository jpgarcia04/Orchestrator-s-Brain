//! # obrain-link — M3 · Relacionar sin IA
//!
//! El hito más subestimado del roadmap. Convierte el log plano que produce M1
//! en un grafo con densidad real, y lo hace con **cero llamadas a un modelo**.
//!
//! Todas las señales que explota ya están en el log o son metadatos del sistema
//! de archivos (`PROJECT-MASTER.md` §4.4). Es el 80% del valor inicial del grafo
//! por una fracción del coste de cualquier cosa que involucre IA.
//!
//! ## La regla que gobierna este crate: **hubs, no cliques**
//!
//! Un grupo de N archivos relacionados **nunca** se conecta todos-con-todos.
//! Eso son N(N−1)/2 aristas: 200 archivos de una misma sesión producirían
//! 19.900 aristas que no dicen nada que no dijera una sola.
//!
//! En su lugar se crea un **nodo concentrador** —una carpeta, una sesión, un
//! canónico— y se enlazan N−1 aristas contra él. El grafo queda legible, la
//! travesía acotada, y el nodo nuevo es además *conocimiento que el sistema
//! autoró* — justo lo que mide la métrica de §0 del maestro.

use obrain_core::{EntityId, Method, NewAssertion, Result, Store, TimePoint, Value};
use std::collections::HashMap;

pub mod dentro;
pub mod duplicates;
pub mod provenance;
pub mod sessions;
pub mod structure;
pub mod versions;

/// Proyección en memoria de un archivo, montada una sola vez para que los
/// enlazadores no repitan consultas.
#[derive(Debug, Clone)]
pub struct FileView {
    pub entity: EntityId,
    pub path: String,
    pub hash: Option<String>,
    pub modified_ms: i64,
    pub size: u64,
    pub availability: String,
}

impl FileView {
    pub fn file_name(&self) -> &str {
        self.path.rsplit(['\\', '/']).next().unwrap_or(&self.path)
    }

    pub fn parent(&self) -> &str {
        match self.path.rfind(['\\', '/']) {
            Some(i) => &self.path[..i],
            None => "",
        }
    }

    /// Sólo los archivos locales tienen bytes en disco. Leer un flujo alternativo
    /// de uno deshidratado podría provocar tráfico, así que no se toca.
    pub fn is_local(&self) -> bool {
        self.availability == "local"
    }
}

/// Carga todos los archivos del log en una sola pasada: cuatro consultas, no
/// cuatro por archivo.
pub fn load_files(store: &Store, at: TimePoint) -> Result<Vec<FileView>> {
    fn first_by_subject(
        rows: Vec<obrain_core::Assertion>,
    ) -> HashMap<EntityId, obrain_core::Value> {
        let mut m = HashMap::new();
        for a in rows {
            // `query` ordena por precedencia y luego por asserted_at DESC,
            // así que la primera de cada sujeto es la vigente más reciente.
            m.entry(a.subject).or_insert(a.object);
        }
        m
    }

    let paths = first_by_subject(store.query(None, Some("path_observed_at"), at)?);
    let hashes = first_by_subject(store.query(None, Some("has_hash"), at)?);
    let mtimes = first_by_subject(store.query(None, Some("modified_at"), at)?);
    let sizes = first_by_subject(store.query(None, Some("file_size"), at)?);
    let avail = first_by_subject(store.query(None, Some("availability"), at)?);

    let mut out = Vec::with_capacity(paths.len());
    for (entity, path) in paths {
        let Some(path) = path.as_text().map(str::to_string) else {
            continue;
        };
        out.push(FileView {
            hash: hashes
                .get(&entity)
                .and_then(|v| v.as_text())
                .map(str::to_string),
            modified_ms: match mtimes.get(&entity) {
                Some(Value::Timestamp(t)) => *t,
                _ => 0,
            },
            size: match sizes.get(&entity) {
                Some(Value::Number(n)) => *n as u64,
                _ => 0,
            },
            availability: avail
                .get(&entity)
                .and_then(|v| v.as_text())
                .unwrap_or("local")
                .to_string(),
            entity,
            path,
        });
    }
    Ok(out)
}

#[derive(Debug, Default)]
pub struct LinkReport {
    pub duplicate_of: usize,
    pub contained_by: usize,
    pub downloaded_from: usize,
    pub occurred_during: usize,
    pub version_of: usize,

    /// Nodos concentradores creados: carpetas y sesiones.
    /// **Esto es conocimiento que el sistema autoró**, no que el usuario escribió.
    pub hubs_created: usize,
    /// Aristas que ya existían. Los enlazadores son idempotentes.
    pub already_present: usize,
    pub errors: Vec<String>,
}

impl LinkReport {
    pub fn edges_total(&self) -> usize {
        self.duplicate_of
            + self.contained_by
            + self.downloaded_from
            + self.occurred_during
            + self.version_of
    }
}

/// Una arista a punto de escribirse.
pub(crate) struct Edge<'a> {
    pub subject: &'a EntityId,
    pub predicate: &'a str,
    pub object: Value,
    pub method: Method,
    pub confidence: f64,
    /// Quién la produjo: `linker:hash`, `heuristic:filename-version`…
    /// Es lo que permite auditar y desactivar un enlazador concreto.
    pub source: &'a str,
}

/// Escribe una arista si no existe ya. Devuelve `true` si escribió.
pub(crate) fn write_edge(
    store: &Store,
    report: &mut LinkReport,
    edge: Edge<'_>,
    at: i64,
) -> Result<bool> {
    let tp = TimePoint::at(at);
    if store.has_assertion(edge.subject, edge.predicate, &edge.object, tp)? {
        report.already_present += 1;
        return Ok(false);
    }
    store.assert(
        NewAssertion::new(
            edge.subject.clone(),
            edge.predicate,
            edge.object,
            edge.method,
            at,
        )
        .confidence(edge.confidence)
        .source(edge.source),
    )?;
    Ok(true)
}

/// Nombre legible de una entidad: su archivo, su carpeta, su sesión, o el id.
///
/// Se afinó en M5a porque un mapa hace de esto un problema visible: los nodos
/// concentradores son los más grandes de la pantalla, y `EXPERIENCE.md` §1 dice
/// que la luz es información. Un nodo enorme llamado `<01KZHYVT>` es el
/// contraejemplo perfecto.
/// El nombre de un archivo: la hoja de su ruta.
fn de_ruta(t: &str) -> String {
    t.rsplit(['\\', '/']).next().unwrap_or(t).to_string()
}

/// El nombre de una carpeta: **su nombre**, y nada más.
///
/// Durante M3–M5 fueron dos segmentos (`usuario/Documentos`) para que cinco
/// carpetas llamadas `src` no fueran cinco nodos indistinguibles. El remedio
/// era peor que la enfermedad: pagaban el prefijo **todas** las carpetas para
/// arreglar las pocas que chocan, y el mapa, las migas y el explorador
/// repetían el nombre del padre en cada escalón. Ahora el choque se resuelve
/// donde ocurre —en la vista que enseña dos carpetas hermanas con el mismo
/// nombre, ver `obrain_api::sky`— y el resto se llama como se llama.
fn de_carpeta(t: &str) -> String {
    let limpio = t.trim_end_matches(['\\', '/']);
    match limpio.rsplit(['\\', '/']).next() {
        Some(hoja) if !hoja.is_empty() => hoja.to_string(),
        _ => t.to_string(),
    }
}

/// Las bibliotecas: las carpetas que el usuario eligió indexar, con su ruta.
///
/// Ordenadas por ruta, para que dos llamadas devuelvan siempre lo mismo.
pub fn libraries(store: &Store, at: TimePoint) -> Result<Vec<(EntityId, String)>> {
    let mut out = Vec::new();
    for a in store.query(None, Some("has_facet"), at)? {
        if a.object.as_text() != Some(obrain_core::Facet::Library.as_str()) {
            continue;
        }
        if let Some(p) = store.resolve(&a.subject, "folder_path", at)? {
            if let Some(t) = p.object.as_text() {
                out.push((a.subject, t.to_string()));
            }
        }
    }
    out.sort_by(|x, y| x.1.cmp(&y.1));
    out.dedup_by(|x, y| x.0 == y.0);
    Ok(out)
}

/// Los nombres de todo, leídos **de una vez**.
///
/// [`label`] resuelve un predicado por entidad y eso es lo correcto para nombrar
/// treinta regiones. Nombrando filas de una lista es ruinoso: el explorador de
/// [ADR-0014] tardaba **1.231 ms** en abrir una carpeta de 232 archivos y la
/// mayor parte era esto, una consulta por nombre.
///
/// Misma regla —las funciones de formato son literalmente las mismas—, una sola
/// pasada por el log. Lo que no cubre son las sesiones: su nombre sale de
/// `session_span`, que vive en el eje de **validez** y no lo encuentra una
/// consulta al presente. Para ésas sigue estando [`label`], y son una por
/// pantalla, no doscientas.
///
/// [ADR-0014]: ../../../docs/adr/0014-dos-vistas-un-solo-estado.md
pub struct Labels {
    ruta: HashMap<EntityId, String>,
    carpeta: HashMap<EntityId, String>,
}

impl Labels {
    pub fn load(store: &Store, at: TimePoint) -> Result<Self> {
        let mut ruta = HashMap::new();
        for a in store.query(None, Some("path_observed_at"), at)? {
            if let Some(t) = a.object.as_text() {
                ruta.entry(a.subject).or_insert_with(|| de_ruta(t));
            }
        }
        let mut carpeta = HashMap::new();
        for a in store.query(None, Some("folder_path"), at)? {
            if let Some(t) = a.object.as_text() {
                carpeta.entry(a.subject).or_insert_with(|| de_carpeta(t));
            }
        }
        Ok(Labels { ruta, carpeta })
    }

    /// El nombre, o `None` si esta entidad no se nombra por ruta ni por carpeta
    /// — una sesión, por ejemplo. Devolver `Option` y no un `<01KZW5…>` de
    /// relleno es lo que permite a quien llama pedirle el nombre a [`label`] en
    /// vez de enseñar un identificador.
    pub fn get(&self, e: &EntityId) -> Option<&str> {
        self.ruta
            .get(e)
            .or_else(|| self.carpeta.get(e))
            .map(String::as_str)
    }
}

pub fn label(store: &Store, e: &EntityId, at: TimePoint) -> String {
    if let Ok(Some(a)) = store.resolve(e, "path_observed_at", at) {
        if let Some(t) = a.object.as_text() {
            return de_ruta(t);
        }
    }
    if let Ok(Some(a)) = store.resolve(e, "folder_path", at) {
        if let Some(t) = a.object.as_text() {
            return de_carpeta(t);
        }
    }
    // Una colección no tiene ruta: se llama como la llamaste (ADR-0031). Si
    // la borraste, como se llamaba.
    let coleccion = store
        .resolve(e, "collection_name", at)
        .ok()
        .flatten()
        .or_else(|| {
            store
                .resolve_last(e, "collection_name", at.known)
                .ok()
                .flatten()
        });
    if let Some(t) = coleccion.as_ref().and_then(|a| a.object.as_text()) {
        return t.to_string();
    }
    // Un documento de la papelera ya no vive en ninguna ruta, pero se sigue
    // llamando como se llamaba (ADR-0025).
    if let Ok(Some(a)) = store.resolve_last(e, "path_observed_at", at.known) {
        if let Some(t) = a.object.as_text() {
            return de_ruta(t);
        }
    }
    // `session_span` vive en el eje de **validez** —la sesión ocurrió entonces,
    // no ahora— así que `resolve` con el presente no la encuentra nunca. Hay
    // que preguntar por lo que el sistema sostiene, no por lo que es cierto hoy.
    if let Ok(Some(a)) = store.resolve_whenever(e, "session_span", at.known) {
        if let Value::Timestamp(t) = a.object {
            return format!("sesión {}", fecha(t));
        }
    }
    format!("<{}>", &e.as_str()[..8.min(e.as_str().len())])
}

/// `epoch ms` → `AAAA-MM-DD`.
///
/// A mano y sin dependencia: es lo único que este crate necesita de un
/// calendario, y `chrono` entero para formatear una fecha en una etiqueta no
/// se paga. Algoritmo civil-from-days de Howard Hinnant.
pub fn fecha(ms: i64) -> String {
    let dias = ms.div_euclid(86_400_000) + 719_468;
    let era = if dias >= 0 { dias } else { dias - 146_096 } / 146_097;
    let doe = dias - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Muestra aristas reales de un predicado, para poder auditar el grafo en vez
/// de confiar en los contadores. Un enlazador que no se puede inspeccionar es
/// un enlazador en el que no se puede confiar.
pub fn sample_edges(
    store: &Store,
    predicate: &str,
    limit: usize,
    at: TimePoint,
) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for a in store
        .query(None, Some(predicate), at)?
        .into_iter()
        .take(limit)
    {
        let from = label(store, &a.subject, at);
        let to = match &a.object {
            Value::Entity(e) => label(store, e, at),
            Value::Text(t) => t.chars().take(64).collect(),
            other => format!("{other:?}"),
        };
        out.push(format!("{from}  ->  {to}   [{:.2}]", a.confidence));
    }
    Ok(out)
}

/// Ejecuta todos los enlazadores. El orden importa poco salvo que la estructura
/// de carpetas se construye primero, porque crea nodos que los demás pueden usar.
pub fn link_all(store: &Store, files: &[FileView], at: i64) -> Result<LinkReport> {
    let mut report = LinkReport::default();
    structure::link(store, files, at, &mut report)?;
    duplicates::link(store, files, at, &mut report)?;
    provenance::link(store, files, at, &mut report)?;
    sessions::link(store, files, at, &mut report)?;
    versions::link(store, files, at, &mut report)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: i64 = 1_700_000_000_000;

    fn nombre_de(store: &Store, e: &EntityId) -> String {
        label(store, e, TimePoint::at(T + 1))
    }

    /// **Regresión de M5a.** Un nodo de sesión salía como `<01KZHYVT>` porque
    /// su intervalo vive en el eje de validez y ya había pasado. Los nodos de
    /// sesión son los más grandes del mapa; sin nombre, el canal visual más
    /// llamativo no dice nada.
    #[test]
    fn una_sesion_tiene_nombre_aunque_su_intervalo_ya_haya_pasado() {
        let store = Store::open_in_memory().unwrap();
        let s = store.create_entity(T).unwrap();
        store
            .assert(
                NewAssertion::new(
                    s.clone(),
                    "session_span",
                    Value::Timestamp(1_754_524_800_000), // 2025-08-07
                    Method::Deterministic,
                    T,
                )
                .valid(Some(1_754_524_800_000), Some(1_754_528_400_000)),
            )
            .unwrap();

        assert_eq!(nombre_de(&store, &s), "sesión 2025-08-07");
    }

    /// Una carpeta se llama como se llama. El choque entre hermanas con el
    /// mismo nombre lo resuelve la vista que las enseña juntas, no el nombre.
    #[test]
    fn una_carpeta_se_nombra_con_su_nombre() {
        let store = Store::open_in_memory().unwrap();
        let c = store.create_entity(T).unwrap();
        store
            .assert(NewAssertion::new(
                c.clone(),
                "folder_path",
                "C:\\repo\\core\\obrain-api\\src",
                Method::Deterministic,
                T,
            ))
            .unwrap();

        assert_eq!(nombre_de(&store, &c), "src");
    }

    /// Un separador final no deja la carpeta sin nombre.
    #[test]
    fn un_separador_final_no_borra_el_nombre() {
        assert_eq!(de_carpeta("C:\\Bóveda\\"), "Bóveda");
        assert_eq!(de_carpeta("/home/jp/Segundo Cerebro"), "Segundo Cerebro");
        assert_eq!(de_carpeta("C:"), "C:");
    }

    /// Un archivo se sigue nombrando con su nombre a secas: ahí el padre es
    /// ruido, porque la ruta completa ya se enseña aparte.
    #[test]
    fn un_archivo_se_nombra_con_su_nombre() {
        let store = Store::open_in_memory().unwrap();
        let f = store.create_entity(T).unwrap();
        store
            .assert(NewAssertion::new(
                f.clone(),
                "path_observed_at",
                "C:\\repo\\core\\obrain-api\\src\\lens.rs",
                Method::Deterministic,
                T,
            ))
            .unwrap();

        assert_eq!(nombre_de(&store, &f), "lens.rs");
    }

    #[test]
    fn la_fecha_sale_del_epoch_sin_dependencias() {
        assert_eq!(fecha(0), "1970-01-01");
        assert_eq!(fecha(1_754_524_800_000), "2025-08-07");
        // Año bisiesto y frontera de siglo, que es donde fallan los cálculos a mano.
        assert_eq!(fecha(951_782_400_000), "2000-02-29");
    }
}
