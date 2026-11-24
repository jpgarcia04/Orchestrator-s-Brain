//! **Las tres herramientas de la puerta de Aetheris**, en el núcleo
//! (ADR-0038, «Qué hay que construir»; ADR-0039, paso 3).
//!
//! La puerta todavía no existe como programa: esto es lo que ese programa
//! llamará, y lo que decide qué puede salir. Cada función trabaja sobre un
//! [`Store`] abierto para leer ([`crate::lectura::abrir_para_leer`] y
//! [`obrain_core::Lectura::leer`]) y nunca mira el PIN ni la ventana: por la
//! puerta no mira JP (ADR-0038, ajuste 3). Lo que no es normal, publicado en
//! su versión actual, con ruta vigente, identidad confirmada y sin texto igual
//! al de algo protegido no entra en ninguna lista, ni cuenta: es
//! [`Elegibles::SoloNormal`], aplicado dentro de cada consulta antes de cortar.
//!
//! - [`localizar`] devuelve [`Ubicacion`]es: **un tipo sin ningún campo donde
//!   quepa texto de un documento** (B4, como el `Signal` de ADR-0006). Es lo
//!   único que Aetheris enseñaría a un modelo remoto. La ruta va relativa a su
//!   biblioteca (ajuste 4).
//! - [`leer`] devuelve pasajes de lo normal, cada uno con su documento, el
//!   método que lo encontró y dos fechas (B7, ajuste 8).
//! - [`documento`] devuelve la ficha de un documento normal, o
//!   [`NoSale::FueraDeLaPuerta`], **lo mismo** para uno sensible, de bóveda, sin
//!   clasificar, en la papelera o inventado (ajuste 2). La ficha no trae
//!   identificadores de aserción ni nada corregible, ni las relaciones
//!   inferidas (`version_of`, `occurred_during`): la versión `1` del contrato
//!   las deja fuera (ADR-0039, condición 2), y meterlas ahora que se
//!   reconcilian sube esa versión. Tampoco el nombre de un relacionado que
//!   no pueda salir.
//!
//! Cada respuesta lleva [`AVISO`], el mismo para cualquier consulta: un aviso
//! que dependiera de la pregunta ya diría algo de lo protegido (ajuste 1).
//!
//! Antes de mirar nada, cada herramienta lee el [ajuste](Posicion) en la misma
//! lectura y, si no la deja, contesta [`NoSale::Apagada`] (ADR-0039, paso 4).
//! Está en el núcleo y no en el programa de la puerta para que no haya una
//! forma de llamar a una herramienta saltándoselo.

use crate::colecciones;
use obrain_core::rusqlite::{OptionalExtension, named_params};
use obrain_core::{Elegibles, EntityId, Store, TimePoint};
use obrain_embed::embedder::Embedder;

/// El aviso fijo de cada respuesta (ADR-0038, ajuste 1). No cambia con la
/// consulta ni con lo que haya en la base.
pub const AVISO: &str = "La puerta de Brain sólo consulta lo normal ya clasificado. No consulta \
lo sensible, lo de bóveda ni lo que está sin clasificar: puede haber más en Brain.";

/// Cuántos resultados como mucho pide una llamada, aunque pida más.
pub const LIMITE_MAXIMO: usize = 50;

/// Pasajes por documento en [`leer`].
const PASAJES_POR_DOCUMENTO: usize = 2;

/// Un documento localizado. **Ningún campo lleva texto del documento**: sólo
/// lo que dice dónde está y por qué salió. Una prueba recorre su
/// serialización.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Ubicacion {
    pub id: String,
    /// El nombre del archivo.
    pub nombre: String,
    /// El nombre de la carpeta de la biblioteca, no su ruta.
    pub biblioteca: String,
    /// La ruta dentro de la biblioteca, con `/`.
    pub ruta: String,
    /// La carpeta dentro de la biblioteca, con `/`; vacía en la raíz.
    pub carpeta: String,
    /// La fecha de modificación que Brain observó en el disco, en ms.
    pub modificado_ms: Option<i64>,
    pub bytes: Option<u64>,
    /// Los nombres de las colecciones donde lo metió JP.
    pub colecciones: Vec<String>,
    /// La rama por la que salió (`bm25` o `knn`) y su puesto en ella.
    pub ramas: Vec<(String, usize)>,
}

/// La respuesta de [`localizar`].
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Localizado {
    pub resultados: Vec<Ubicacion>,
    /// Si la búsqueda por significado corrió; sin modelo, sólo léxica.
    pub semantica: bool,
    pub aviso: &'static str,
}

/// Un pasaje literal de un documento normal.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Pasaje {
    pub documento: String,
    pub nombre: String,
    pub texto: String,
    /// Qué lo encontró: `bm25` (las palabras) o `knn` (el significado).
    pub metodo: &'static str,
    /// Cuándo leyó Brain esta versión del archivo, en ms.
    pub leido_ms: Option<i64>,
    /// La fecha de modificación que Brain observó en el disco, en ms.
    pub modificado_ms: Option<i64>,
}

/// La respuesta de [`leer`].
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Leido {
    pub pasajes: Vec<Pasaje>,
    pub semantica: bool,
    pub aviso: &'static str,
}

/// La ficha de un documento normal.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Ficha {
    pub id: String,
    pub nombre: String,
    pub biblioteca: String,
    pub ruta: String,
    /// La ruta completa, para abrir el archivo. Sólo aquí, nunca en
    /// [`Ubicacion`] (ajuste 4).
    pub ruta_completa: String,
    pub modificado_ms: Option<i64>,
    pub bytes: Option<u64>,
    pub colecciones: Vec<String>,
    /// La fecha de caducidad que Brain cree que tiene, con su procedencia.
    pub vence: Option<Afirmacion>,
    pub relaciones: Vec<Relacion>,
    pub aviso: &'static str,
}

/// Una afirmación con su procedencia, sin identificador ni evidencia.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Afirmacion {
    pub valor: String,
    pub metodo: String,
    pub fuente: String,
    pub confianza: f64,
}

/// Una relación determinista con algo que también puede salir.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Relacion {
    /// `carpeta`, `copia_de` o `dentro_de`.
    pub tipo: &'static str,
    /// El documento relacionado; vacío para una carpeta.
    pub id: String,
    pub nombre: String,
    /// Su ruta dentro de la biblioteca.
    pub ruta: String,
}

/// Por qué una herramienta no da lo pedido sin que sea un fallo. La puerta lo
/// sirve como `no_disponible` con su [`motivo`](NoSale::motivo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoSale {
    /// El ajuste no deja usar esta herramienta: la puerta está apagada o, para
    /// [`leer`] y [`documento`], en «sólo localizar» (ADR-0038, ajuste 5).
    Apagada,
    /// La única respuesta de [`documento`] para todo lo que no es un documento
    /// normal y publicado: no dice cuál de los casos es (ajuste 2).
    FueraDeLaPuerta,
}

impl NoSale {
    pub fn motivo(self) -> &'static str {
        match self {
            NoSale::Apagada => "apagada",
            NoSale::FueraDeLaPuerta => "fuera_de_la_puerta",
        }
    }
}

impl std::fmt::Display for NoSale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.motivo())
    }
}

impl std::error::Error for NoSale {}

/// **El ajuste de la puerta**, con sus tres posiciones (ADR-0038, B2 y ajuste
/// 5). De fábrica, apagada.
///
/// Se guarda en la tabla `seguridad`, dentro de la base cifrada, junto al PIN:
/// es un control, no conocimiento, y por eso no va al log de aserciones (ver
/// [`crate::seguridad::migrate`]). La puerta lo lee con su apertura de sólo
/// lectura, dentro de la misma lectura que contesta, así que no hay forma de
/// que una respuesta mezcle un ajuste y datos de momentos distintos. Sólo la
/// ventana lo escribe ([`crate::Brain::set_gate_position`]); la puerta no
/// tiene con qué.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Posicion {
    /// Ninguna herramienta contesta.
    Apagada,
    /// Sólo [`localizar`]: nombres y rutas, ningún texto.
    SoloLocalizar,
    /// Las tres: también pasajes y fichas de lo normal.
    LocalizarYLeer,
}

impl Posicion {
    /// La clave en la tabla `seguridad`.
    pub const CLAVE: &'static str = "puerta_de_aetheris";

    pub fn as_str(self) -> &'static str {
        match self {
            Posicion::Apagada => "apagada",
            Posicion::SoloLocalizar => "solo_localizar",
            Posicion::LocalizarYLeer => "localizar_y_leer",
        }
    }

    /// La posición que dice este texto, o `None` si no es ninguna.
    pub fn desde(texto: &str) -> Option<Self> {
        [
            Posicion::Apagada,
            Posicion::SoloLocalizar,
            Posicion::LocalizarYLeer,
        ]
        .into_iter()
        .find(|p| p.as_str() == texto)
    }

    /// `Ok` si esta posición deja contestar a `herramienta`.
    fn deja(self, herramienta: Herramienta) -> Result<(), NoSale> {
        let minima = match herramienta {
            Herramienta::Localizar => Posicion::SoloLocalizar,
            Herramienta::Leer | Herramienta::Documento => Posicion::LocalizarYLeer,
        };
        if self >= minima {
            Ok(())
        } else {
            Err(NoSale::Apagada)
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Herramienta {
    Localizar,
    Leer,
    Documento,
}

/// La posición guardada. **Sin ajuste, o con uno que no se entiende, la
/// puerta está apagada**: un valor que no es ninguna posición no puede abrir
/// nada, venga de un error o de una versión que no es ésta.
pub fn posicion(store: &Store) -> crate::Result<Posicion> {
    Ok(
        crate::seguridad::leer_ajuste(store.conn(), Posicion::CLAVE)?
            .and_then(|v| Posicion::desde(&v))
            .unwrap_or(Posicion::Apagada),
    )
}

/// Escribe la posición. Sólo [`crate::Brain::set_gate_position`], que decide
/// cuándo hace falta el PIN, la llama.
pub(crate) fn poner_posicion(store: &Store, p: Posicion) -> crate::Result<()> {
    crate::seguridad::poner_ajuste(store.conn(), Posicion::CLAVE, p.as_str())?;
    Ok(())
}

/// Un resultado de búsqueda antes de vestirlo: documento, ruta y por dónde
/// salió.
struct Encontrado {
    entity: EntityId,
    path: String,
    ramas: Vec<(String, usize)>,
    chunk: Option<i64>,
}

/// Las dos ramas con `SoloNormal`; sin modelo, sólo la léxica.
fn buscar(
    store: &Store,
    embedder: Option<&dyn Embedder>,
    consulta: &str,
    limite: usize,
    at: TimePoint,
) -> crate::Result<(Vec<Encontrado>, bool)> {
    let limite = limite.min(LIMITE_MAXIMO);
    Ok(match embedder {
        Some(e) => {
            let (fused, _, _) = obrain_embed::search::hybrid_eligible(
                store,
                e,
                consulta,
                limite,
                at,
                None,
                Elegibles::SoloNormal,
            )?;
            let v = fused
                .into_iter()
                .map(|h| Encontrado {
                    entity: h.entity,
                    path: h.path,
                    ramas: h.ranks,
                    chunk: h.chunk,
                })
                .collect();
            (v, true)
        }
        None => {
            let v = obrain_extract::search_eligible(
                store,
                consulta,
                limite,
                at,
                None,
                Elegibles::SoloNormal,
            )?
            .into_iter()
            .enumerate()
            .map(|(i, h)| Encontrado {
                entity: h.entity,
                path: h.path,
                ramas: vec![("bm25".to_string(), i + 1)],
                chunk: None,
            })
            .collect();
            (v, false)
        }
    })
}

/// Dónde cae `path` dentro de las bibliotecas: el nombre de la biblioteca y
/// la ruta relativa, con `/`. `None` si no está en ninguna; entonces no sale,
/// porque no hay ruta relativa que dar y la completa no puede salir.
fn en_biblioteca(bibliotecas: &[String], path: &str) -> Option<(String, String)> {
    let normal = |p: &str| p.replace('\\', "/");
    let ruta = normal(path);
    bibliotecas
        .iter()
        .map(|b| normal(b).trim_end_matches('/').to_string())
        .filter(|b| !b.is_empty() && ruta.len() > b.len() + 1)
        .filter(|b| ruta.starts_with(b.as_str()) && ruta.as_bytes()[b.len()] == b'/')
        .max_by_key(String::len)
        .map(|b| {
            let nombre = b.rsplit('/').next().unwrap_or_default().to_string();
            (nombre, ruta[b.len() + 1..].to_string())
        })
}

fn bibliotecas(store: &Store, at: TimePoint) -> crate::Result<Vec<String>> {
    Ok(obrain_link::libraries(store, at)?
        .into_iter()
        .map(|(_, ruta)| ruta)
        .collect())
}

fn marca_de_tiempo(store: &Store, e: &EntityId, predicado: &str, at: TimePoint) -> Option<i64> {
    let a = store.resolve(e, predicado, at).ok()??;
    match a.object {
        obrain_core::Value::Timestamp(t) => Some(t),
        obrain_core::Value::Number(n) if n.is_finite() => Some(n as i64),
        _ => None,
    }
}

fn bytes(store: &Store, e: &EntityId, at: TimePoint) -> Option<u64> {
    let a = store.resolve(e, "file_size", at).ok()??;
    match a.object {
        obrain_core::Value::Number(n) if n.is_finite() && n >= 0.0 => Some(n as u64),
        _ => None,
    }
}

fn colecciones_de(store: &Store, e: &EntityId, at: TimePoint) -> crate::Result<Vec<String>> {
    Ok(colecciones::de(store, e, at)?
        .into_iter()
        .map(|p| p.name)
        .collect())
}

/// ¿Puede salir esta entidad por la puerta? La misma condición que filtra las
/// búsquedas, sobre una sola entidad.
fn puede_salir(store: &Store, e: &EntityId, at: TimePoint) -> crate::Result<bool> {
    let sql = format!(
        "SELECT 1 FROM entities x WHERE x.id = :id AND {}",
        Elegibles::SoloNormal.sql("x.id")
    );
    Ok(store
        .conn()
        .query_row(
            &sql,
            named_params! {":id": e.as_str(), ":valid": at.valid, ":known": at.known},
            |_| Ok(()),
        )
        .optional()
        .map_err(obrain_core::Error::from)?
        .is_some())
}

/// Biblioteca, ruta relativa y carpeta relativa de `path`.
fn ubicar(libs: &[String], path: &str) -> Option<(String, String, String)> {
    let (biblioteca, ruta) = en_biblioteca(libs, path)?;
    let carpeta = ruta
        .rsplit_once('/')
        .map(|(c, _)| c.to_string())
        .unwrap_or_default();
    Some((biblioteca, ruta, carpeta))
}

/// **`localizar`**: nombres y rutas de lo normal que casa con la consulta, sin
/// ningún texto de los documentos. Ver el módulo.
pub fn localizar(
    store: &Store,
    embedder: Option<&dyn Embedder>,
    consulta: &str,
    limite: usize,
    at: TimePoint,
) -> crate::Result<Result<Localizado, NoSale>> {
    if let Err(no) = posicion(store)?.deja(Herramienta::Localizar) {
        return Ok(Err(no));
    }
    let (encontrados, semantica) = buscar(store, embedder, consulta, limite, at)?;
    let libs = bibliotecas(store, at)?;
    let mut resultados = Vec::with_capacity(encontrados.len());
    for f in encontrados {
        let Some((biblioteca, ruta, carpeta)) = ubicar(&libs, &f.path) else {
            continue;
        };
        resultados.push(Ubicacion {
            nombre: obrain_link::label(store, &f.entity, at),
            biblioteca,
            ruta,
            carpeta,
            modificado_ms: marca_de_tiempo(store, &f.entity, "modified_at", at),
            bytes: bytes(store, &f.entity, at),
            colecciones: colecciones_de(store, &f.entity, at)?,
            ramas: f.ramas,
            id: f.entity.0,
        });
    }
    Ok(Ok(Localizado {
        resultados,
        semantica,
        aviso: AVISO,
    }))
}

/// Cuándo extrajo Brain el texto que hoy publica de esta entidad.
fn leido_ms(store: &Store, e: &EntityId) -> crate::Result<Option<i64>> {
    Ok(store
        .conn()
        .query_row(
            "SELECT extracted_at FROM extractions
              WHERE entity = ?1 AND chunker = ?2 AND chunker_ver = ?3 AND status = 'ok'",
            obrain_core::rusqlite::params![
                e.as_str(),
                obrain_extract::chunk::CHUNKER,
                obrain_extract::chunk::CHUNKER_VERSION
            ],
            |r| r.get(0),
        )
        .optional()
        .map_err(obrain_core::Error::from)?)
}

/// **`leer`**: pasajes literales de lo normal que casa con la consulta, cada
/// uno con su documento, su método y sus dos fechas. Ver el módulo.
pub fn leer(
    store: &Store,
    embedder: Option<&dyn Embedder>,
    consulta: &str,
    limite: usize,
    at: TimePoint,
) -> crate::Result<Result<Leido, NoSale>> {
    if let Err(no) = posicion(store)?.deja(Herramienta::Leer) {
        return Ok(Err(no));
    }
    let (encontrados, semantica) = buscar(store, embedder, consulta, limite, at)?;
    let libs = bibliotecas(store, at)?;
    let mut pasajes = Vec::new();
    for f in encontrados {
        if en_biblioteca(&libs, &f.path).is_none() {
            continue;
        }
        let nombre = obrain_link::label(store, &f.entity, at);
        let leido = leido_ms(store, &f.entity)?;
        let modificado = marca_de_tiempo(store, &f.entity, "modified_at", at);
        // Primero lo que encuentran las palabras; después, si quedó hueco, el
        // fragmento que encontró el significado. Así cada pasaje dice de
        // verdad qué lo trajo.
        let lexicos = obrain_extract::search::passages_for(
            store,
            &f.entity,
            consulta,
            None,
            PASAJES_POR_DOCUMENTO,
        )?;
        let todos = match f.chunk {
            Some(_) => obrain_extract::search::passages_for(
                store,
                &f.entity,
                consulta,
                f.chunk,
                PASAJES_POR_DOCUMENTO,
            )?,
            None => lexicos.clone(),
        };
        for p in todos {
            let metodo = if lexicos.iter().any(|l| l.span == p.span) {
                "bm25"
            } else {
                "knn"
            };
            pasajes.push(Pasaje {
                documento: f.entity.0.clone(),
                nombre: nombre.clone(),
                texto: p.text,
                metodo,
                leido_ms: leido,
                modificado_ms: modificado,
            });
        }
    }
    Ok(Ok(Leido {
        pasajes,
        semantica,
        aviso: AVISO,
    }))
}

/// **`documento`**: la ficha de un documento normal, o
/// [`NoSale::FueraDeLaPuerta`] para todo lo demás, sin decir qué es. Ver el
/// módulo.
pub fn documento(store: &Store, id: &str, at: TimePoint) -> crate::Result<Result<Ficha, NoSale>> {
    if let Err(no) = posicion(store)?.deja(Herramienta::Documento) {
        return Ok(Err(no));
    }
    let e = EntityId(id.to_string());
    if !puede_salir(store, &e, at)? {
        return Ok(Err(NoSale::FueraDeLaPuerta));
    }
    let libs = bibliotecas(store, at)?;
    let Some(ruta_completa) = store
        .resolve(&e, "path_observed_at", at)?
        .and_then(|a| a.object.as_text().map(str::to_string))
    else {
        return Ok(Err(NoSale::FueraDeLaPuerta));
    };
    let Some((biblioteca, ruta, _)) = ubicar(&libs, &ruta_completa) else {
        return Ok(Err(NoSale::FueraDeLaPuerta));
    };

    let vence = store.resolve(&e, "expires_on", at)?.and_then(|a| {
        let valor = match &a.object {
            obrain_core::Value::Text(t) => t.clone(),
            obrain_core::Value::Timestamp(t) => t.to_string(),
            _ => return None,
        };
        Some(Afirmacion {
            valor,
            metodo: a.method.as_str().to_string(),
            fuente: a.source.clone(),
            confianza: a.confidence,
        })
    });

    let mut relaciones = Vec::new();
    for (predicado, tipo) in [
        ("contained_by", "carpeta"),
        ("duplicate_of", "copia_de"),
        (obrain_core::PART_OF, "dentro_de"),
    ] {
        for a in store.query(Some(&e), Some(predicado), at)? {
            let Some(destino) = a.object.as_entity() else {
                continue;
            };
            if tipo == "carpeta" {
                let Some(ruta_carpeta) = store
                    .resolve(destino, "folder_path", at)?
                    .and_then(|f| f.object.as_text().map(str::to_string))
                else {
                    continue;
                };
                // Una carpeta fuera de las bibliotecas no tiene ruta relativa:
                // la raíz de la biblioteca misma se nombra sin ruta.
                let (nombre, ruta) = match en_biblioteca(&libs, &ruta_carpeta) {
                    Some((_, r)) => (r.rsplit('/').next().unwrap_or_default().to_string(), r),
                    None if libs.iter().any(|b| {
                        b.replace('\\', "/").trim_end_matches('/')
                            == ruta_carpeta.replace('\\', "/").trim_end_matches('/')
                    }) =>
                    {
                        (biblioteca.clone(), String::new())
                    }
                    None => continue,
                };
                relaciones.push(Relacion {
                    tipo,
                    id: String::new(),
                    nombre,
                    ruta,
                });
                continue;
            }
            // Un relacionado sólo se nombra si él también puede salir.
            if !puede_salir(store, destino, at)? {
                continue;
            }
            let Some(ruta_destino) = store
                .resolve(destino, "path_observed_at", at)?
                .and_then(|p| p.object.as_text().map(str::to_string))
            else {
                continue;
            };
            let Some((_, ruta)) = en_biblioteca(&libs, &ruta_destino) else {
                continue;
            };
            if relaciones
                .iter()
                .any(|r: &Relacion| r.tipo == tipo && r.id == destino.0)
            {
                continue;
            }
            relaciones.push(Relacion {
                tipo,
                id: destino.0.clone(),
                nombre: obrain_link::label(store, destino, at),
                ruta,
            });
        }
    }

    Ok(Ok(Ficha {
        nombre: obrain_link::label(store, &e, at),
        biblioteca,
        ruta,
        ruta_completa,
        modificado_ms: marca_de_tiempo(store, &e, "modified_at", at),
        bytes: bytes(store, &e, at),
        colecciones: colecciones_de(store, &e, at)?,
        vence,
        relaciones,
        aviso: AVISO,
        id: e.0,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cada_posicion_deja_sus_herramientas_y_ninguna_mas() {
        use Herramienta::*;
        let deja = |p: Posicion| [Localizar, Leer, Documento].map(|h| p.deja(h).is_ok());
        assert_eq!(deja(Posicion::Apagada), [false, false, false]);
        assert_eq!(deja(Posicion::SoloLocalizar), [true, false, false]);
        assert_eq!(deja(Posicion::LocalizarYLeer), [true, true, true]);
        assert_eq!(Posicion::Apagada.deja(Leer), Err(NoSale::Apagada));
    }

    #[test]
    fn la_posicion_se_escribe_y_se_lee_con_el_mismo_texto() {
        for p in [
            Posicion::Apagada,
            Posicion::SoloLocalizar,
            Posicion::LocalizarYLeer,
        ] {
            assert_eq!(Posicion::desde(p.as_str()), Some(p));
            // La ventana la recibe y la manda en JSON con el mismo texto.
            let json = serde_json::to_string(&p).unwrap();
            assert_eq!(json, format!("\"{}\"", p.as_str()));
            assert_eq!(serde_json::from_str::<Posicion>(&json).unwrap(), p);
        }
        assert_eq!(Posicion::desde("Apagada"), None);
        assert_eq!(NoSale::Apagada.motivo(), "apagada");
        assert_eq!(NoSale::FueraDeLaPuerta.motivo(), "fuera_de_la_puerta");
    }

    #[test]
    fn la_ruta_relativa_es_la_de_la_biblioteca_mas_profunda() {
        let libs = vec![
            "C:\\Users\\ana\\Documentos".to_string(),
            "C:\\Users\\ana\\Documentos\\Trámites".to_string(),
        ];
        assert_eq!(
            en_biblioteca(
                &libs,
                "C:\\Users\\ana\\Documentos\\Trámites\\2026\\visa.pdf"
            ),
            Some(("Trámites".into(), "2026/visa.pdf".into()))
        );
        assert_eq!(
            en_biblioteca(&libs, "C:\\Users\\ana\\Documentos\\notas.txt"),
            Some(("Documentos".into(), "notas.txt".into()))
        );
        // Un prefijo de texto que no es una carpeta no cuenta.
        assert_eq!(
            en_biblioteca(&libs, "C:\\Users\\ana\\DocumentosViejos\\x.txt"),
            None
        );
        assert_eq!(en_biblioteca(&libs, "D:\\otra\\x.txt"), None);
    }
}
