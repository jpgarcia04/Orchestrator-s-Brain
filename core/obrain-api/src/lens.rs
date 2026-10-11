//! La lente del invariante I2: **foco + radio + presupuesto duro de nodos**.
//!
//! > *El grafo es una lente sobre una consulta, nunca un volcado.*
//!
//! Es la primera vez que alguien **lee** el grafo que M3 lleva escribiendo
//! desde el tercer hito. Hasta ahora `obrain-link` producía aristas y sólo las
//! miraba `--sample`, de doce en doce.
//!
//! ## Por qué el presupuesto es duro y no un `LIMIT`
//!
//! Un `LIMIT` corta por donde caiga la consulta. Aquí el corte es **por capas**:
//! se admite el foco entero, luego sus vecinos, luego los vecinos de esos, y se
//! para cuando el presupuesto se acaba. Un vecino directo del foco nunca se
//! queda fuera para hacer sitio a uno de segundo grado, que es exactamente lo
//! que haría un `LIMIT` sobre una travesía en profundidad.
//!
//! Y cuando corta, **lo dice** ([`Lens::truncated`]). Un mapa que esconde que
//! está enseñando una parte es un mapa que miente sobre el tamaño de tu vida.
//!
//! ## Los canales visuales se resuelven aquí, no en la interfaz
//!
//! `EXPERIENCE.md` §1 fija que la luminosidad codifica relevancia, la
//! saturación confianza, el trazo el método y el tamaño la densidad de
//! conexiones. Los cuatro datos salen de este módulo ya calculados —
//! `distance`, `confidence`, `method`, `degree`— porque son **semántica del
//! conocimiento, no decisiones de dibujo**. Una interfaz que los dedujera por
//! su cuenta sería una segunda definición de qué significa "relevante", y la
//! primera vez que las dos discreparan el mapa mentiría sin que nadie lo notara.

use obrain_core::{Assertion, EntityId, Method, Store, TimePoint, Value};
use std::collections::{HashMap, HashSet};

/// Un nodo tal y como se dibuja.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Node {
    pub id: String,
    /// Nombre legible. Lo decide [`obrain_link::label`] y no se decide otra vez.
    pub label: String,
    /// Saltos desde el foco. `0` es el foco. → **luminosidad**.
    pub distance: u8,
    /// Aristas vivas que tocan este nodo **en todo el grafo**, no sólo las
    /// visibles. → **tamaño**.
    ///
    /// Global a propósito: si el tamaño dependiera de lo que cabe en la
    /// pantalla, el mismo nodo cambiaría de tamaño al cambiar de consulta y la
    /// memoria espacial —que es todo el argumento de `EXPERIENCE.md` §6— no
    /// funcionaría.
    pub degree: usize,
    pub facets: Vec<String>,
    /// `normal` | `sensitive` | `vault`. La interfaz tiene que poder marcar lo
    /// que la puerta retiene sin volver a preguntar por qué.
    pub sensitivity: &'static str,
    /// Posición de partida en el disco unidad, derivada del identificador.
    ///
    /// Es la mitad barata de la estabilidad de layout que pide
    /// `EXPERIENCE.md` §6: la misma entidad arranca siempre en el mismo sitio,
    /// hoy y dentro de tres años, sin guardar nada. La otra mitad —que la
    /// simulación de fuerzas converja igual— es del cliente, y sólo puede
    /// cumplirla si parte de aquí.
    pub seed: [f32; 2],
    /// Cuándo se tocó por última vez, en epoch ms. `0` si no se sabe.
    ///
    /// → **movimiento residual** (§1): *«lo tocado hace poco tiene un latido
    /// casi imperceptible; lo antiguo está quieto»*. Es el canal que hace real
    /// la palabra «respirar» de la visión original, y con función.
    pub recency: i64,
}

/// Una arista tal y como se dibuja.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub predicate: String,
    /// `deterministic` → trazo sólido · `inferred` → punteado (§1 de EXPERIENCE).
    pub method: &'static str,
    /// → **opacidad de la arista**.
    pub confidence: f64,
    /// Quién la produjo: `linker:hash`, `heuristic:filename-version`…
    /// Es la procedencia de I9 llevada hasta el píxel.
    pub source: String,
}

/// Lo que se dibuja: nunca el grafo entero.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Lens {
    pub focus: Vec<String>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// El presupuesto que se pidió.
    pub budget: usize,
    /// El presupuesto cortó y hay más grafo del que se enseña.
    pub truncated: bool,
    /// Cuántos nodos había disponibles cuando se cortó. Con `truncated`, es la
    /// diferencia entre "esto es todo" y "esto es una parte".
    pub reached: usize,
}

/// Presupuesto por defecto. I2 sitúa la lente entre ~50 y ~500 nodos; el modo
/// Foco de `EXPERIENCE.md` §4.3 pide ~300.
pub const BUDGET: usize = 300;

/// Radio por defecto: el modo Foco es "una entidad al centro, radio 2".
pub const RADIUS: u8 = 2;

/// Presupuesto del **sustrato**: el mapa atenuado que reacciona mientras se
/// escribe (ADR-0004), no el modo Foco.
///
/// Más bajo que [`BUDGET`] por dos razones que apuntan al mismo sitio. La de
/// diseño: lo que se ve de reojo tiene que decir *dónde* cae la respuesta, y
/// para eso trescientos puntos son ruido. Y la medida: el layout es O(n²) por
/// iteración, así que a 300 nodos un redibujado cuesta **~200 ms** — aceptable
/// una vez al enfocar, inaceptable en cada pulsación. A 120 baja a ~30 ms, por
/// debajo de la espera de la barra.
pub const SUBSTRATE_BUDGET: usize = 120;

/// Un salto del grafo, ya normalizado a `(vecino, arista)` mire hacia donde mire.
struct Incident {
    assertion: obrain_core::AssertionId,
    other: EntityId,
    edge: Edge,
}

/// Todas las aristas vivas que tocan `e`, en las dos direcciones.
///
/// Las dos direcciones no son simetría gratuita: una arista se almacena **una
/// sola vez** (§3.4), así que un nodo concentrador de M3 —una carpeta, una
/// sesión— es el *objeto* de sus 40 aristas y no el sujeto de ninguna. Mirar
/// sólo hacia delante lo dejaría aislado justo a él, que es el nodo que más
/// significa.
///
/// Lo que está en la papelera (`fuera`) no es vecino de nadie: ya no está en tu
/// bóveda (ADR-0025). Sus aristas siguen en el log.
fn incidents(
    store: &Store,
    e: &EntityId,
    at: TimePoint,
    fuera: &HashSet<EntityId>,
) -> obrain_core::Result<Vec<Incident>> {
    let mut out = Vec::new();
    let mut push = |a: Assertion, from: EntityId, to: EntityId, other: EntityId| {
        if fuera.contains(&other) {
            return;
        }
        out.push(Incident {
            assertion: a.id,
            other,
            edge: Edge {
                from: from.0,
                to: to.0,
                predicate: a.predicate,
                method: a.method.as_str(),
                confidence: a.confidence,
                source: a.source,
            },
        });
    };

    for a in store.query(Some(e), None, at)? {
        if let Value::Entity(o) = a.object.clone() {
            push(a, e.clone(), o.clone(), o);
        }
    }
    for a in store.query_by_object(e, at)? {
        let s = a.subject.clone();
        push(a, s.clone(), e.clone(), s);
    }
    Ok(out)
}

/// Abre la lente sobre `focus`.
///
/// Recorre por capas hasta `radius` saltos y para en `budget` nodos. El orden
/// dentro de una capa es determinista —confianza descendente, y el
/// identificador para desempatar— así que **la misma consulta devuelve
/// exactamente el mismo subgrafo**. Es el prototipo #2 de `EXPERIENCE.md` §8
/// resuelto en el servidor: si el conjunto de nodos bailara entre dos
/// ejecuciones, ninguna estabilidad de layout en el cliente podría arreglarlo.
pub fn open(
    store: &Store,
    focus: &[EntityId],
    radius: u8,
    budget: usize,
    at: TimePoint,
) -> obrain_core::Result<Lens> {
    let mut admitted: HashMap<EntityId, u8> = HashMap::new();
    let mut order: Vec<EntityId> = Vec::new();
    let mut edges: HashMap<obrain_core::AssertionId, Edge> = HashMap::new();
    let mut degree: HashMap<EntityId, usize> = HashMap::new();
    let mut truncated = false;
    let mut reached: HashSet<EntityId> = HashSet::new();
    let pendientes = crate::identity::pending_set(store, at)?;
    let mut fuera = obrain_scan::ausentes::missing_set(store, at)?;
    fuera.extend(pendientes.iter().cloned());

    // ── capa 0 · el foco ──
    let mut layer: Vec<EntityId> = Vec::new();
    for e in focus {
        if pendientes.contains(e) {
            continue;
        }
        if admitted.contains_key(e) {
            continue;
        }
        reached.insert(e.clone());
        if admitted.len() >= budget {
            truncated = true;
            continue;
        }
        admitted.insert(e.clone(), 0);
        order.push(e.clone());
        layer.push(e.clone());
    }

    // ── capas 1..=radius ──
    for d in 0..radius {
        // Candidatos de la capa siguiente, con la mejor confianza con la que
        // se llega a cada uno: es lo que decide quién entra si el presupuesto
        // se queda corto.
        let mut candidates: HashMap<EntityId, f64> = HashMap::new();

        for e in &layer {
            for inc in incidents(store, e, at, &fuera)? {
                *degree.entry(e.clone()).or_insert(0) += 1;
                edges.entry(inc.assertion).or_insert(inc.edge.clone());
                reached.insert(inc.other.clone());
                if admitted.contains_key(&inc.other) {
                    continue;
                }
                let best = candidates.entry(inc.other).or_insert(f64::MIN);
                *best = best.max(inc.edge.confidence);
            }
        }

        // Determinismo: por confianza descendente y, en empate, por
        // identificador. El ULID es estable, así que dos ejecuciones sobre la
        // misma base admiten exactamente los mismos nodos.
        let mut next: Vec<(EntityId, f64)> = candidates.into_iter().collect();
        next.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        layer.clear();
        for (e, _) in next {
            if admitted.len() >= budget {
                truncated = true;
                break;
            }
            admitted.insert(e.clone(), d + 1);
            order.push(e.clone());
            layer.push(e);
        }
        if layer.is_empty() {
            break;
        }
    }

    // La última capa no se expandió, así que aún no tiene grado. Se le pregunta
    // igual: el tamaño de un nodo del borde tiene que decir cuánto grafo hay
    // detrás de él, que es justo la pista de por dónde seguir.
    for e in &layer {
        let inc = incidents(store, e, at, &fuera)?;
        degree.insert(e.clone(), inc.len());
        for i in inc {
            reached.insert(i.other);
            edges.entry(i.assertion).or_insert(i.edge);
        }
    }

    // ── proyectar a lo que se dibuja ──
    let nodes = order
        .iter()
        .map(|e| {
            Ok(Node {
                label: obrain_link::label(store, e, at),
                distance: admitted[e],
                degree: degree.get(e).copied().unwrap_or(0),
                facets: store.facets(e, at)?,
                sensitivity: store.sensitivity(e).map(|s| s.as_str()).unwrap_or("vault"),
                seed: seed_position(e.as_str()),
                recency: recency(store, e, at),
                id: e.0.clone(),
            })
        })
        .collect::<obrain_core::Result<Vec<_>>>()?;

    // Una arista sólo se dibuja si sus **dos** extremos están dentro. Las que
    // salen del presupuesto se descartan enteras en vez de dibujarse colgando
    // hacia la nada: un cabo suelto se lee como un nodo que falta, y el nodo no
    // falta, es que no cabía.
    let inside: HashSet<&str> = admitted.keys().map(|e| e.as_str()).collect();
    let mut edges: Vec<Edge> = edges
        .into_values()
        .filter(|e| inside.contains(e.from.as_str()) && inside.contains(e.to.as_str()))
        .collect();
    edges.sort_by(|a, b| (&a.from, &a.to, &a.predicate).cmp(&(&b.from, &b.to, &b.predicate)));

    Ok(Lens {
        focus: focus
            .iter()
            .filter(|e| !pendientes.contains(*e))
            .map(|e| e.0.clone())
            .collect(),
        nodes,
        edges,
        budget,
        truncated,
        reached: reached.len(),
    })
}

/// Cuándo se tocó esto por última vez, en epoch ms. `0` si no se sabe.
///
/// Un archivo lo dice en `modified_at`. Una sesión lo dice en su intervalo —y
/// hay que preguntarlo con `resolve_whenever`, porque ese intervalo vive en el
/// eje de validez y ya pasó—. Una carpeta no lo dice: se queda en `0`, que es
/// «no lo sé» y se dibuja neutro. Inventarle la fecha de su contenido costaría
/// una consulta por nodo y sería una cifra que el log no sostiene.
fn recency(store: &Store, e: &EntityId, at: TimePoint) -> i64 {
    if let Ok(Some(a)) = store.resolve(e, "modified_at", at) {
        if let Value::Timestamp(t) = a.object {
            return t;
        }
    }
    if let Ok(Some(a)) = store.resolve_whenever(e, "session_span", at.known) {
        if let Value::Timestamp(t) = a.object {
            return t;
        }
    }
    0
}

/// Posición estable en el disco unidad, derivada del identificador.
///
/// FNV-1a y no un hash criptográfico: aquí no se defiende nada, sólo hace falta
/// que dos ejecuciones den lo mismo y que identificadores parecidos no acaben
/// amontonados. Que es justo lo que pasa sin el paso final de abajo.
///
/// **FNV-1a solo no vale, y lo destapó dibujarlo.** Los últimos bytes de la
/// entrada pasan por una sola multiplicación, así que apenas se propagan: dos
/// cadenas que difieren en el último carácter salen con los 20 bits altos
/// idénticos y caen prácticamente encima. Importa aquí porque los ULID
/// **comparten prefijo por construcción** —los primeros diez caracteres son la
/// marca de tiempo— y toda la diferencia entre dos entidades creadas en el
/// mismo escaneo vive al final de la cadena, que es exactamente la parte que
/// FNV-1a no mezcla.
///
/// El remate es el finalizador de `splitmix64`: tres desplazamientos y dos
/// multiplicaciones que sí propagan cada bit a todos los demás.
pub(crate) fn seed_position(id: &str) -> [f32; 2] {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in id.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h ^= h >> 30;
    h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^= h >> 31;

    let angle = (h as u32 as f32 / u32::MAX as f32) * std::f32::consts::TAU;
    // Raíz cuadrada: sin ella los puntos se apelmazan en el centro, porque el
    // área de un anillo crece con el radio.
    let r = ((h >> 32) as u32 as f32 / u32::MAX as f32).sqrt();
    [r * angle.cos(), r * angle.sin()]
}

/// Cuántas aristas vivas hay en total. Un número que hasta ahora sólo existía
/// en el informe de M3, sumando los contadores de cada enlazador.
pub fn edge_census(store: &Store, at: TimePoint) -> obrain_core::Result<Vec<(String, usize)>> {
    let mut by_predicate: HashMap<String, usize> = HashMap::new();
    for a in store.query(None, None, at)? {
        if matches!(a.object, Value::Entity(_)) {
            *by_predicate.entry(a.predicate).or_insert(0) += 1;
        }
    }
    let mut out: Vec<(String, usize)> = by_predicate.into_iter().collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Ok(out)
}

/// Las entidades con más aristas vivas. Es `--sample` para el mapa: enseña qué
/// va a salir grande **antes** de dibujarlo.
pub fn hubs(
    store: &Store,
    limit: usize,
    at: TimePoint,
) -> obrain_core::Result<Vec<(String, usize)>> {
    let pendientes = crate::identity::pending_set(store, at)?;
    let mut degree: HashMap<EntityId, usize> = HashMap::new();
    for a in store.query(None, None, at)? {
        if let Value::Entity(o) = &a.object {
            if pendientes.contains(&a.subject) || pendientes.contains(o) {
                continue;
            }
            *degree.entry(a.subject.clone()).or_insert(0) += 1;
            *degree.entry(o.clone()).or_insert(0) += 1;
        }
    }
    let mut out: Vec<(EntityId, usize)> = degree.into_iter().collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out.truncate(limit);
    Ok(out
        .into_iter()
        .map(|(e, n)| (obrain_link::label(store, &e, at), n))
        .collect())
}

/// El método de una aserción, para quien sólo necesita el trazo.
pub fn stroke(method: Method) -> &'static str {
    match method {
        Method::Deterministic | Method::User => "solid",
        _ => "dashed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Los ULID de un mismo escaneo comparten los diez primeros caracteres,
    /// que son la marca de tiempo. Toda su diferencia está al final — y el
    /// final es justo lo que FNV-1a no propaga sin un paso de remate.
    ///
    /// Sin el finalizador de `splitmix64` este test da distancias del orden de
    /// 0,001: doscientas entidades del mismo escaneo arrancarían todas en el
    /// mismo punto, la repulsión tendría que deshacerlo desde cero y el layout
    /// dejaría de ser estable en la práctica aunque lo fuera en el papel.
    #[test]
    fn los_identificadores_con_prefijo_comun_no_se_amontonan() {
        const PREFIJO: &str = "01JQ8XZ2K5T0000000000";
        let puntos: Vec<[f32; 2]> = (0..40)
            .map(|i| seed_position(&format!("{PREFIJO}{i:03}")))
            .collect();

        let mut minima = f32::MAX;
        for (i, a) in puntos.iter().enumerate() {
            for b in &puntos[i + 1..] {
                minima = minima.min(((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt());
            }
        }
        assert!(
            minima > 0.01,
            "cuarenta entidades del mismo escaneo se solapan: separacion minima {minima}"
        );
    }

    /// La estabilidad de `EXPERIENCE.md` §6 no depende de guardar nada: el
    /// mismo identificador da el mismo punto siempre.
    #[test]
    fn la_posicion_solo_depende_del_identificador() {
        assert_eq!(seed_position("01JQ8XZ2K5T"), seed_position("01JQ8XZ2K5T"));
        assert_ne!(seed_position("01JQ8XZ2K5T"), seed_position("01JQ8XZ2K5U"));
    }

    /// Dentro del disco unidad, siempre. El cliente multiplica por un radio en
    /// píxeles y da por hecho que la entrada está acotada.
    #[test]
    fn la_posicion_cae_dentro_del_disco_unidad() {
        for i in 0..500 {
            let [x, y] = seed_position(&format!("entidad-{i}"));
            assert!(
                (x * x + y * y).sqrt() <= 1.0 + 1e-6,
                "fuera del disco: {x},{y}"
            );
        }
    }
}
