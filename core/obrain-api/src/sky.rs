//! **El cielo**: el grafo entero leído a poca escala.
//!
//! `EXPERIENCE.md` §6 lo pide por su nombre y lo llama una de las dos cosas que
//! hay que diseñar *ahora* para sobrevivir al arco de años:
//!
//! > A poco zoom no ves 40.000 nodos: **ves 30 regiones con nombre.** Te acercas
//! > y una región se despliega.
//!
//! Es la otra lente. [`crate::lens`] responde *«esto y lo que tiene al lado»*;
//! ésta responde *«qué hay, en total»*. Una galaxia no se dibuja con la primera:
//! foco más radio enseña un vecindario, y un vecindario no es un cielo.
//!
//! ## Regiones y agrupaciones son el mismo árbol a dos alturas
//!
//! No hay dos ideas aquí, hay una leída a dos zooms. El árbol de contención que
//! escribió M3 —archivo → carpeta → carpeta padre— se corta alto para el cielo
//! ([`regions`]) y al ras para agrupar resultados ([`cluster`]). Que sean la
//! misma estructura es lo que impide que el mapa y la lista discrepen sobre a
//! qué pertenece un archivo.
//!
//! ## Las sesiones no son lugares: son puentes
//!
//! El árbol de carpetas es **dónde** están las cosas. Las sesiones de trabajo de
//! M3 son **cuándo** se tocaron, y mezclar los dos ejes en una vista da un mapa
//! que no se puede leer.
//!
//! Así que una sesión que tocó archivos de dos regiones distintas no es un nodo:
//! es una **arista entre esas dos regiones**. Es la forma exacta que
//! `EXPERIENCE.md` §6 llama «Puentes» en el mes 1–3, y sale gratis de datos que
//! M3 lleva escribiendo desde el tercer hito.

use obrain_core::{EntityId, Result, Store, TimePoint, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// Una región del cielo. Una «estrella».
#[derive(Debug, Clone, serde::Serialize)]
pub struct Region {
    pub id: String,
    pub label: String,
    /// Ruta completa. La etiqueta corta desambigua entre hermanas; ésta dice
    /// dónde está de verdad.
    pub path: String,
    /// Archivos que contiene, contando los de sus subcarpetas.
    pub weight: usize,
    /// Profundidad en el árbol de carpetas. La escala a la que se abrió.
    pub depth: u8,
    /// Esta región se abrió en trozos y esto es **sólo lo que cuelga
    /// directamente de ella**, no su subárbol.
    pub remainder: bool,
    pub seed: [f32; 2],
    /// El nivel más restrictivo que hay dentro. Una región con algo en la
    /// bóveda se marca entera: es lo que permite saber dónde no mirar.
    pub sensitivity: &'static str,
    /// Lo más reciente que hay dentro, en epoch ms. `0` si no se sabe.
    ///
    /// Es el dato que sostiene la frase del README —*«las zonas brillantes son
    /// donde tu vida está activa ahora mismo»*— y por eso está aquí y no en la
    /// interfaz. `EXPERIENCE.md` §1 no admite un canal visual que no codifique
    /// algo: una región cálida lo es porque la tocaste ayer, no porque quede
    /// bien.
    pub recency: i64,
}

/// Dos regiones que una sesión de trabajo cruzó.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Bridge {
    pub from: String,
    pub to: String,
    /// Cuántas sesiones distintas las cruzaron. → grosor.
    pub weight: usize,
}

/// El cielo entero.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Sky {
    pub regions: Vec<Region>,
    /// Los puentes más fuertes, hasta el presupuesto. Ver [`Sky::bridges_found`].
    pub bridges: Vec<Bridge>,
    /// Cuántos había antes de recortar.
    ///
    /// **El recorte no es cosmético.** Una jornada de trabajo toca media vida a
    /// la vez, así que casi cualquier par de regiones acaba unido por alguna
    /// sesión: sobre este repositorio salían 41 puentes para 66 parejas
    /// posibles, es decir un ovillo que dice «todo se relaciona con todo», que
    /// es lo mismo que no decir nada. Se enseñan los más fuertes y se dice
    /// cuántos había, igual que hace la lente con los nodos (I2).
    pub bridges_found: usize,
    /// Archivos en el log.
    pub files: usize,
    /// De ésos, cuántos caen dentro de alguna región.
    pub covered: usize,
    /// Cuántas regiones se pidieron.
    pub target: usize,
    /// El rango temporal del corpus, para normalizar la recencia sin que el
    /// cliente tenga que adivinar contra qué compararla.
    pub oldest: i64,
    pub newest: i64,
}

/// Cuántas regiones enseñar por defecto.
///
/// §6 dice «30 regiones con nombre». Es el número de cosas que una persona
/// abarca de un vistazo, no un límite técnico — el renderizador aguanta miles
/// (prototipo `01-graph-render`) y aun así ése no es el criterio.
pub const REGIONS: usize = 30;

/// Cuántas regiones tiene sentido abrir para este corpus.
///
/// `target` es un **techo, no una cuota**. Treinta regiones son las adecuadas
/// para decenas de miles de archivos; para cien son treinta etiquetas encima de
/// carpetas de tres archivos, que es exactamente el ruido que la agregación
/// existe para evitar.
///
/// La raíz cuadrada es la elección de siempre para repartir en cubos, y aquí
/// además da la curva que se quiere: 100 archivos → 10 regiones, 10.000 → 30
/// (el techo), y el mapa crece por **densidad** en vez de por número, que es lo
/// que pide `EXPERIENCE.md` §6 — *el presupuesto de nodos no crece: crece la
/// agregación*.
fn effective(target: usize, files: usize) -> usize {
    (((files as f64).sqrt().ceil() as usize).min(target)).max(1)
}

// ───────────────────────────────────────────────────── el árbol de contención

/// El árbol que escribió M3, leído de una vez.
struct Tree {
    /// Carpetas: identificador → ruta.
    folders: HashMap<EntityId, String>,
    parent: HashMap<EntityId, EntityId>,
    children: BTreeMap<EntityId, Vec<EntityId>>,
    /// Archivos que cuelgan directamente de cada carpeta.
    direct: BTreeMap<EntityId, Vec<EntityId>>,
    /// Archivos del subárbol, acumulado.
    weight: HashMap<EntityId, usize>,
    /// Carpeta inmediata de cada archivo.
    home: HashMap<EntityId, EntityId>,
    files: usize,
}

fn read_tree(store: &Store, at: TimePoint) -> Result<Tree> {
    let mut folders: HashMap<EntityId, String> = HashMap::new();
    for a in store.query(None, Some("folder_path"), at)? {
        if let Some(p) = a.object.as_text() {
            folders.entry(a.subject).or_insert_with(|| p.to_string());
        }
    }

    let mut parent = HashMap::new();
    let mut children: BTreeMap<EntityId, Vec<EntityId>> = BTreeMap::new();
    let mut direct: BTreeMap<EntityId, Vec<EntityId>> = BTreeMap::new();
    let mut home = HashMap::new();

    for a in store.query(None, Some("contained_by"), at)? {
        let Value::Entity(up) = a.object else { continue };
        if folders.contains_key(&a.subject) {
            // carpeta dentro de carpeta
            parent.insert(a.subject.clone(), up.clone());
            children.entry(up).or_default().push(a.subject);
        } else {
            home.insert(a.subject.clone(), up.clone());
            direct.entry(up).or_default().push(a.subject);
        }
    }
    for v in children.values_mut() {
        v.sort();
    }
    for v in direct.values_mut() {
        v.sort();
    }

    // Peso transitivo, de abajo arriba. Iterativo y no recursivo: una jerarquía
    // de carpetas de verdad puede tener treinta niveles y una pila reventada
    // durante un escaneo no es un error que se pueda explicar.
    let mut weight: HashMap<EntityId, usize> = HashMap::new();
    let roots: Vec<EntityId> = folders
        .keys()
        .filter(|f| !parent.contains_key(*f))
        .cloned()
        .collect();
    let mut order: Vec<EntityId> = Vec::new();
    let mut stack = roots.clone();
    while let Some(h) = stack.pop() {
        order.push(h.clone());
        if let Some(cs) = children.get(&h) {
            stack.extend(cs.iter().cloned());
        }
    }
    for h in order.iter().rev() {
        let own = direct.get(h).map_or(0, Vec::len);
        let sub: usize = children
            .get(h)
            .map_or(0, |cs| cs.iter().map(|c| weight.get(c).copied().unwrap_or(0)).sum());
        weight.insert(h.clone(), own + sub);
    }

    let files = home.len();
    Ok(Tree { folders, parent, children, direct, weight, home, files })
}

impl Tree {
    fn depth<'a>(&'a self, mut h: &'a EntityId) -> u8 {
        let mut d = 0u8;
        while let Some(up) = self.parent.get(h) {
            d = d.saturating_add(1);
            h = up;
            if d == u8::MAX {
                break;
            }
        }
        d
    }
}

// ─────────────────────────────────────────────────────────────── las regiones

/// Cómo se está mirando una carpeta.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Zoom {
    /// Ella y todo lo que cuelga.
    Subtree,
    /// Sólo lo que cuelga directamente: el resto ya se abrió en trozos.
    Remainder,
}

/// Abre el cielo: parte el árbol hasta tener `target` regiones.
///
/// **Se parte por el mayor, repetidamente.** Empieza por las raíces —una sola
/// estrella enorme que no dice nada— y sustituye la región más gorda por sus
/// hijas, otra vez y otra, hasta llegar al número pedido. Es exactamente el
/// gesto de acercarse en un mapa: lo grande se abre antes que lo pequeño, y lo
/// que ya es pequeño se queda entero.
///
/// La alternativa —coger las N carpetas con más archivos— da siempre el mismo
/// resultado inútil: la raíz, y dentro de ella la misma raíz otra vez, porque
/// una carpeta que sólo tiene una hija pesa lo mismo que su hija.
///
/// Determinista: el desempate es por identificador, así que dos ejecuciones
/// sobre la misma base abren exactamente el mismo cielo.
pub fn regions(store: &Store, target: usize, at: TimePoint) -> Result<Sky> {
    let tree = read_tree(store, at)?;

    let mut open: Vec<(EntityId, Zoom)> = tree
        .folders
        .keys()
        .filter(|f| !tree.parent.contains_key(*f))
        .cloned()
        .map(|f| (f, Zoom::Subtree))
        .collect();
    open.sort_by(|a, b| a.0.cmp(&b.0));

    let peso = |h: &EntityId, z: Zoom| match z {
        Zoom::Subtree => tree.weight.get(h).copied().unwrap_or(0),
        Zoom::Remainder => tree.direct.get(h).map_or(0, Vec::len),
    };

    let cuantas = effective(target, tree.files);
    while open.len() < cuantas {
        // La mayor que todavía se pueda abrir.
        let Some(i) = open
            .iter()
            .enumerate()
            .filter(|(_, (h, z))| {
                *z == Zoom::Subtree && tree.children.get(h).is_some_and(|c| !c.is_empty())
            })
            .max_by(|(_, (a, za)), (_, (b, zb))| {
                peso(a, *za).cmp(&peso(b, *zb)).then_with(|| b.cmp(a))
            })
            .map(|(i, _)| i)
        else {
            break;
        };

        let (h, _) = open.remove(i);
        let mut hijas: Vec<&EntityId> = tree.children.get(&h).into_iter().flatten().collect();
        hijas.sort_by(|a, b| {
            tree.weight
                .get(*b)
                .cmp(&tree.weight.get(*a))
                .then_with(|| a.cmp(b))
        });

        // **No se promueven todas.** Una carpeta de fotos con doscientos años
        // dentro convertiría un solo paso en doscientas estrellas. Se ascienden
        // las mayores hasta llenar el techo y el resto se queda agregado bajo la
        // madre, que es lo que hace un mapa al alejarse: unas cuantas ciudades
        // con nombre y el campo alrededor.
        let sitio = cuantas.saturating_sub(open.len());
        let todas = hijas.len() <= sitio;
        let promovidas = if todas { hijas.len() } else { sitio.saturating_sub(1) };

        for c in hijas.iter().take(promovidas) {
            open.push(((*c).clone(), Zoom::Subtree));
        }
        // Lo que no subió —archivos propios de la madre y las hijas que no
        // cupieron— sigue existiendo y no puede quedarse sin sitio.
        let queda = !todas || tree.direct.get(&h).is_some_and(|d| !d.is_empty());
        if queda {
            open.push((h, Zoom::Remainder));
        }
    }

    // ── a qué región pertenece cada archivo ──
    //
    // El reparto va **antes** de construir las estrellas, y el peso de cada una
    // sale de contar lo que le tocó. Es la única forma de que no puedan
    // discrepar: con dos cálculos —uno para el tamaño y otro para el reparto—
    // llegaría el día en que una estrella se dibuja grande y al abrirla no hay
    // nada dentro.
    let abiertas: HashSet<EntityId> = open.iter().map(|(h, _)| h.clone()).collect();
    let de_quien = assign(&tree, &abiertas);

    let mut cuenta: HashMap<&EntityId, usize> = HashMap::new();
    for r in de_quien.values() {
        *cuenta.entry(r).or_insert(0) += 1;
    }

    // Lo más reciente de cada región, en una sola consulta. Recorrer el log una
    // vez y repartir es lo mismo que hace `obrain_link::load_files`: una
    // consulta por predicado, no una por archivo.
    let mut reciente: HashMap<&EntityId, i64> = HashMap::new();
    let (mut oldest, mut newest) = (i64::MAX, 0i64);
    for a in store.query(None, Some("modified_at"), at)? {
        let Value::Timestamp(t) = a.object else { continue };
        let Some(r) = de_quien.get(&a.subject) else { continue };
        oldest = oldest.min(t);
        newest = newest.max(t);
        let e = reciente.entry(r).or_insert(0);
        *e = (*e).max(t);
    }
    if oldest == i64::MAX {
        oldest = 0;
    }

    let mut regions: Vec<Region> = Vec::with_capacity(open.len());
    for (h, z) in &open {
        let w = cuenta.get(h).copied().unwrap_or(0);
        if w == 0 {
            continue;
        }
        regions.push(Region {
            label: obrain_link::label(store, h, at),
            path: tree.folders.get(h).cloned().unwrap_or_default(),
            weight: w,
            depth: tree.depth(h),
            remainder: *z == Zoom::Remainder,
            seed: crate::lens::seed_position(h.as_str()),
            sensitivity: "normal",
            recency: reciente.get(h).copied().unwrap_or(0),
            id: h.0.clone(),
        });
    }
    regions.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.id.cmp(&b.id)));

    // La sensibilidad más restrictiva que haya dentro sube a la región.
    let mut peor: HashMap<String, &'static str> = HashMap::new();
    for (f, r) in &de_quien {
        let nivel = store.sensitivity(f).map(|s| s.as_str()).unwrap_or("vault");
        let e = peor.entry(r.0.clone()).or_insert("normal");
        if orden(nivel) > orden(e) {
            *e = nivel;
        }
    }
    for r in &mut regions {
        if let Some(n) = peor.get(&r.id) {
            r.sensitivity = n;
        }
    }

    // ── los puentes: las sesiones que cruzaron regiones ──
    let mut por_sesion: BTreeMap<EntityId, BTreeSet<String>> = BTreeMap::new();
    for a in store.query(None, Some("occurred_during"), at)? {
        let Value::Entity(sesion) = a.object else { continue };
        if let Some(r) = de_quien.get(&a.subject) {
            por_sesion.entry(sesion).or_default().insert(r.0.clone());
        }
    }
    let mut cruces: BTreeMap<(String, String), usize> = BTreeMap::new();
    for tocadas in por_sesion.values() {
        let v: Vec<&String> = tocadas.iter().collect();
        for (i, a) in v.iter().enumerate() {
            for b in &v[i + 1..] {
                *cruces.entry(((*a).clone(), (*b).clone())).or_insert(0) += 1;
            }
        }
    }
    let mut bridges: Vec<Bridge> = cruces
        .into_iter()
        .map(|((from, to), weight)| Bridge { from, to, weight })
        .collect();
    bridges.sort_by(|a, b| {
        b.weight
            .cmp(&a.weight)
            .then_with(|| (&a.from, &a.to).cmp(&(&b.from, &b.to)))
    });
    let bridges_found = bridges.len();
    // Presupuesto duro, como todo lo demás que se dibuja aquí. Uno y medio por
    // región deja ver la estructura sin que las líneas tapen las estrellas.
    bridges.truncate(regions.len() * 3 / 2);

    Ok(Sky {
        covered: de_quien.len(),
        files: tree.files,
        regions,
        bridges,
        bridges_found,
        target,
        oldest,
        newest,
    })
}

fn orden(s: &str) -> u8 {
    match s {
        "vault" => 2,
        "sensitive" => 1,
        _ => 0,
    }
}

/// Archivo → región que lo contiene: el primer antepasado que esté abierto.
///
/// Se sube desde el archivo en vez de bajar desde la región. Bajando habría que
/// decidir qué hacer cuando una región contiene a otra; subiendo, el primero que
/// se encuentra es por construcción el más cercano, y no hay ambigüedad que
/// resolver.
fn assign(tree: &Tree, abiertas: &HashSet<EntityId>) -> HashMap<EntityId, EntityId> {
    let mut out = HashMap::with_capacity(tree.files);
    for (file, casa) in &tree.home {
        let mut h = casa.clone();
        loop {
            if abiertas.contains(&h) {
                out.insert(file.clone(), h);
                break;
            }
            match tree.parent.get(&h) {
                Some(up) => h = up.clone(),
                None => break,
            }
        }
    }
    out
}

// ──────────────────────────────────────────── el mismo árbol, leído al ras

/// Un grupo de resultados: los que comparten carpeta.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Cluster {
    /// La carpeta. `None` para los que no tienen ninguna.
    pub hub: Option<String>,
    pub label: String,
    /// Identificadores de los resultados, en el orden en que venían.
    pub members: Vec<String>,
    /// La cadena de carpetas por encima de ésta, de dentro hacia fuera.
    ///
    /// Es lo que permite encender la región del cielo donde cae este grupo sin
    /// recalcular el cielo entero en cada pulsación: el cliente ya lo tiene
    /// cargado y sólo necesita saber de qué rama cuelga cada resultado.
    pub ancestors: Vec<String>,
    /// El mejor puesto que alcanzó cualquiera de sus miembros. Ordena los
    /// grupos sin inventar una puntuación de grupo: si el resultado número uno
    /// está aquí dentro, este grupo va primero.
    pub best: usize,
}

/// La cadena de carpetas por encima de una, de dentro hacia fuera.
///
/// Con tope, y no por prudencia decorativa: la travesía sube por aristas del
/// log, y una arista de contención mal formada —o un enlace simbólico que
/// alguien escanee algún día— convertiría esto en un bucle infinito dentro de
/// una consulta que se ejecuta en cada pulsación.
fn ancestry(store: &Store, from: &EntityId, at: TimePoint) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut vistos = HashSet::new();
    let mut cur = from.clone();
    while out.len() < 32 && vistos.insert(cur.clone()) {
        let Some(up) = store
            .resolve(&cur, "contained_by", at)?
            .and_then(|a| a.object.as_entity().cloned())
        else {
            break;
        };
        out.push(up.0.clone());
        cur = up;
    }
    Ok(out)
}

/// Agrupa resultados por la carpeta que comparten.
///
/// Es el mismo árbol de [`regions`] leído al ras en vez de alto. Y es la misma
/// regla que gobierna `obrain-link` desde M3 —**hubs, no cliques**— aplicada por
/// fin a la recuperación: doce archivos sueltos son casi siempre tres cosas, y
/// enseñarlos en fila obliga a la persona a reconstruir a mano una agrupación
/// que el sistema ya conocía.
pub fn cluster(store: &Store, hits: &[crate::Hit], at: TimePoint) -> Result<Vec<Cluster>> {
    let mut orden_de: Vec<(Option<EntityId>, usize, String)> = Vec::with_capacity(hits.len());
    for (i, h) in hits.iter().enumerate() {
        let e = EntityId(h.entity.clone());
        let casa = store
            .resolve(&e, "contained_by", at)?
            .and_then(|a| a.object.as_entity().cloned());
        orden_de.push((casa, i, h.entity.clone()));
    }

    let mut grupos: Vec<Cluster> = Vec::new();
    let mut indice: HashMap<Option<EntityId>, usize> = HashMap::new();
    for (casa, i, id) in orden_de {
        match indice.get(&casa) {
            Some(&g) => grupos[g].members.push(id),
            None => {
                indice.insert(casa.clone(), grupos.len());
                grupos.push(Cluster {
                    label: match &casa {
                        Some(h) => obrain_link::label(store, h, at),
                        None => "sin carpeta".to_string(),
                    },
                    ancestors: match &casa {
                        Some(h) => ancestry(store, h, at)?,
                        None => Vec::new(),
                    },
                    hub: casa.map(|h| h.0),
                    members: vec![id],
                    best: i + 1,
                });
            }
        }
    }
    // `best` ya es el mejor puesto porque se recorre en orden, y el orden de
    // los grupos sale de ahí: no hace falta una puntuación agregada, que
    // siempre acaba siendo una media que no significa nada.
    grupos.sort_by_key(|g| g.best);
    Ok(grupos)
}
