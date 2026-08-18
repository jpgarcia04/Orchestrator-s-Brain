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
    /// Carpetas con la faceta `project`: árboles de trabajo de git, marcados
    /// por M3 al ver un `.git` en el disco.
    projects: HashSet<EntityId>,
    /// Archivos del subárbol **cuyo formato el sistema reconoce**, acumulado.
    /// Cero significa que ahí dentro no hay nada que una persona haya escrito,
    /// fotografiado o grabado. Ver [`Tree::muda`].
    legible: HashMap<EntityId, usize>,
    /// ¿Ha mirado alguien el formato de algo en este corpus?
    ///
    /// Separa **«miré y no encontré nada»** de **«no miré nunca»**, que es la
    /// diferencia entre una conclusión y un vacío. Sin M2 ejecutado no hay un
    /// solo `mime_guess` en el log, y sin esta guarda el cielo entero sería
    /// mudo y no se abriría ninguna región — un índice recién escaneado y sin
    /// extraer se vería igual que uno lleno de basura.
    sabe_formatos: bool,
    files: usize,
}

fn read_tree(store: &Store, at: TimePoint) -> Result<Tree> {
    let mut folders: HashMap<EntityId, String> = HashMap::new();
    for a in store.query(None, Some("folder_path"), at)? {
        if let Some(p) = a.object.as_text() {
            folders.entry(a.subject).or_insert_with(|| p.to_string());
        }
    }

    // Qué archivos tienen un formato reconocido. `mime_guess` lo escribe M2 al
    // triar, así que esto no calcula nada nuevo: lee lo que ya se sabía.
    //
    // **Menos los `.log`**, y no es una lista de exclusiones disfrazada: un
    // registro lo escribe un programa, siempre, por definición — es la única
    // extensión de la que eso se puede afirmar sin mirar dentro. Se descubrió
    // arreglando lo de arriba: al dejar de partirse los repositorios, el reparto
    // bajó dentro de `Rockstar Games`, que escapaba a la regla del mudo gracias a
    // catorce `launcher.NN.log`. Los catorce `.log` con formato de todo el corpus
    // real son esos catorce.
    let mut es_registro: HashSet<EntityId> = HashSet::new();
    for a in store.query(None, Some("path_observed_at"), at)? {
        if a.object.as_text().is_some_and(|p| p.to_ascii_lowercase().ends_with(".log")) {
            es_registro.insert(a.subject);
        }
    }
    let mut con_formato: HashSet<EntityId> = HashSet::new();
    for a in store.query(None, Some("mime_guess"), at)? {
        if !es_registro.contains(&a.subject) {
            con_formato.insert(a.subject);
        }
    }

    let mut projects: HashSet<EntityId> = HashSet::new();
    for a in store.query(None, Some("has_facet"), at)? {
        if a.object.as_text() == Some("project") {
            projects.insert(a.subject);
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
    let mut legible: HashMap<EntityId, usize> = HashMap::new();
    for h in order.iter().rev() {
        let own = direct.get(h).map_or(0, Vec::len);
        let sub: usize = children
            .get(h)
            .map_or(0, |cs| cs.iter().map(|c| weight.get(c).copied().unwrap_or(0)).sum());
        weight.insert(h.clone(), own + sub);

        let own_leg = direct
            .get(h)
            .map_or(0, |ds| ds.iter().filter(|d| con_formato.contains(*d)).count());
        let sub_leg: usize = children
            .get(h)
            .map_or(0, |cs| cs.iter().map(|c| legible.get(c).copied().unwrap_or(0)).sum());
        legible.insert(h.clone(), own_leg + sub_leg);
    }

    let files = home.len();
    let sabe_formatos = !con_formato.is_empty();
    Ok(Tree {
        folders,
        parent,
        children,
        direct,
        weight,
        home,
        projects,
        legible,
        sabe_formatos,
        files,
    })
}

impl Tree {
    /// Una carpeta que **no se parte**: se lleva una estrella y se entra en
    /// ella con el explorador.
    ///
    /// Dos casos, y el segundo sale del primero:
    ///
    /// 1. **Un proyecto de código.** Un repositorio es *una cosa*, no
    ///    doscientas. Su interior es hondo, ancho y uniforme —exactamente el
    ///    árbol que gana el concurso de peso que describe [ADR-0015]— y sus
    ///    dieciséis subcarpetas dicen todas lo mismo: *aquí hay código*.
    /// 2. **Un taller**: una carpeta cuyas hijas son *todas* proyectos.
    ///    `Documentos\GitHub` es eso y nada más, y partirlo devuelve una
    ///    estrella por repositorio en lugar de una por vida.
    ///
    /// **Esto no filtra nada.** Los archivos siguen en el índice, siguen
    /// buscándose y siguen alcanzándose por el explorador; lo único que cambia
    /// es cuántas plazas del cielo se llevan. Es la distinción de
    /// [ADR-0016](../../../docs/adr/0016-la-prueba-se-elige-el-indice-no-se-filtra.md):
    /// se elige qué se enseña, no qué se sabe.
    ///
    /// Y no toca el tamaño, que sigue siendo el número de archivos como fijó
    /// [ADR-0015]: una carpeta atomica se dibuja con todo su peso. Lo que se
    /// decide aquí es si es **divisible**, que es el otro eje de esa misma ADR.
    ///
    /// [ADR-0015]: ../../../docs/adr/0015-el-cielo-reparte-por-anchura-no-por-peso.md
    fn atomica(&self, h: &EntityId) -> bool {
        if self.projects.contains(h) {
            return true;
        }
        match self.children.get(h) {
            Some(cs) if !cs.is_empty() => cs.iter().all(|c| self.projects.contains(c)),
            _ => false,
        }
    }

    /// Una carpeta que **no se gana una estrella**: su subárbol entero son
    /// formatos que el sistema no reconoce.
    ///
    /// No es un juicio sobre la importancia —§1 prohíbe eso— sino sobre si hay
    /// algo ahí que una persona haya escrito, fotografiado o grabado. Un
    /// `PROFSAVE_backup_profile_synced`, un `kv_hubble-uuid` y una `httpcache`
    /// llena de números no son sitios de una vida: son el almacén interno de un
    /// programa, y ocupan una plaza del cielo que le hace falta a otra cosa.
    ///
    /// **La medida que la justifica**, sobre el corpus real: de los 18 archivos
    /// de `Battlefield 1` y los 6 de `My Games`, **ninguno** tiene formato
    /// reconocido. En cambio `GTA Videos` sí —dos `image/jpeg`— porque ahí
    /// están los clips que grabó el usuario, y por eso conserva su estrella.
    /// La regla se apoya en esa asimetría y no en una lista de nombres de
    /// juegos, que volvería a fallar con el juego del mes que viene.
    ///
    /// **No se excluye nada.** Los archivos siguen en el índice y en el
    /// explorador; lo que pierden es el derecho a una estrella. Su peso se
    /// queda agregado en la carpeta de arriba, que sigue cubriéndolos.
    fn muda(&self, h: &EntityId) -> bool {
        self.sabe_formatos && self.legible.get(h).copied().unwrap_or(0) == 0
    }

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
/// Lo que se sabe de una región **además** de cuántos archivos tiene.
///
/// Existe porque el primer corpus real (2026-08-10) dejó claro que el peso por
/// número de archivos no es lo mismo que la importancia: la región mayor del
/// cielo eran 229 capturas de pantalla, y la resolución del IMSS y los estados
/// del banco —la pregunta de la visa, literalmente— caían en regiones de 12 y
/// de 2. Antes de cambiar la regla hay que **mirar** qué señal ordena bien, y
/// eso es lo que esto sirve.
///
/// No trae una puntuación agregada a propósito. Una media de cinco cosas es
/// una opinión con aspecto de dato, y §1 prohíbe exactamente eso.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RegionAudit {
    pub label: String,
    /// La regla de hoy: archivos del subárbol.
    pub files: usize,
    /// Aserciones vivas sobre esos archivos. Cuánto **sabe** el sistema de lo
    /// que hay aquí, que no es lo mismo que cuánto hay.
    pub assertions: usize,
    /// Sesiones de trabajo distintas que tocaron algo de dentro. Cuántas veces
    /// por separado has vuelto aquí.
    pub sessions: usize,
    /// Días entre el archivo más viejo y el más nuevo. Distingue una carpeta
    /// que abarca años de una que se llenó en una tarde.
    pub span_days: i64,
    /// Aristas hacia archivos de **otra** región. Con cuánto del resto de tu
    /// vida se relaciona esto.
    pub out_edges: usize,
}

/// Mide las señales candidatas sobre las mismas regiones que abre [`regions`].
pub fn audit(store: &Store, target: usize, at: TimePoint) -> Result<Vec<RegionAudit>> {
    let sky = regions(store, target, at)?;
    let tree = read_tree(store, at)?;

    // ── agregados por archivo, de una pasada ──
    let mut n_assert: HashMap<String, usize> = HashMap::new();
    {
        let conn = store.conn();
        let mut st = conn.prepare(
            "SELECT subject, COUNT(*) FROM assertions WHERE retracted_at IS NULL GROUP BY subject",
        )?;
        let mut rows = st.query([])?;
        while let Some(r) = rows.next()? {
            n_assert.insert(r.get::<_, String>(0)?, r.get::<_, i64>(1)? as usize);
        }
    }

    let mut sesiones: HashMap<EntityId, Vec<EntityId>> = HashMap::new();
    for a in store.query(None, Some("occurred_during"), at)? {
        if let Value::Entity(s) = a.object {
            sesiones.entry(a.subject).or_default().push(s);
        }
    }

    let mut fecha: HashMap<EntityId, i64> = HashMap::new();
    for a in store.query(None, Some("modified_at"), at)? {
        let Value::Timestamp(t) = a.object else { continue };
        fecha.insert(a.subject, t);
    }

    // Aristas archivo→archivo. `contained_by` es el árbol mismo y
    // `occurred_during` ya se lee como puente: ninguna de las dos dice nada
    // nuevo aquí.
    let mut salidas: Vec<(EntityId, EntityId)> = Vec::new();
    for pred in ["duplicate_of", "version_of", "downloaded_from"] {
        for a in store.query(None, Some(pred), at)? {
            if let Value::Entity(o) = a.object {
                salidas.push((a.subject, o));
            }
        }
    }

    // ── de qué región es cada archivo, según el corte que abrió el cielo ──
    let mut de_region: HashMap<EntityId, usize> = HashMap::new();
    let mut ficheros: Vec<Vec<EntityId>> = Vec::with_capacity(sky.regions.len());
    for (i, reg) in sky.regions.iter().enumerate() {
        let raiz = EntityId(reg.id.clone());
        let mut acc = Vec::new();
        if reg.remainder {
            acc.extend(tree.direct.get(&raiz).into_iter().flatten().cloned());
        } else {
            let mut pila = vec![raiz.clone()];
            while let Some(f) = pila.pop() {
                acc.extend(tree.direct.get(&f).into_iter().flatten().cloned());
                pila.extend(tree.children.get(&f).into_iter().flatten().cloned());
            }
        }
        for f in &acc {
            de_region.insert(f.clone(), i);
        }
        ficheros.push(acc);
    }

    let mut fuera = vec![0usize; sky.regions.len()];
    for (a, b) in &salidas {
        if let (Some(&ra), Some(&rb)) = (de_region.get(a), de_region.get(b)) {
            if ra != rb {
                fuera[ra] += 1;
                fuera[rb] += 1;
            }
        }
    }

    Ok(sky
        .regions
        .iter()
        .enumerate()
        .map(|(i, reg)| {
            let fs = &ficheros[i];
            let mut ses: HashSet<&EntityId> = HashSet::new();
            let (mut min, mut max) = (i64::MAX, 0i64);
            let mut asr = 0usize;
            for f in fs {
                asr += n_assert.get(&f.0).copied().unwrap_or(0);
                if let Some(v) = sesiones.get(f) {
                    ses.extend(v.iter());
                }
                if let Some(&t) = fecha.get(f) {
                    if t > 0 {
                        min = min.min(t);
                        max = max.max(t);
                    }
                }
            }
            RegionAudit {
                label: reg.label.clone(),
                files: reg.weight,
                assertions: asr,
                sessions: ses.len(),
                span_days: if min == i64::MAX { 0 } else { (max - min) / 86_400_000 },
                out_edges: fuera[i],
            }
        })
        .collect())
}

///
/// Determinista: el desempate es por identificador, así que dos ejecuciones
/// sobre la misma base abren exactamente el mismo cielo.
pub fn regions(store: &Store, target: usize, at: TimePoint) -> Result<Sky> {
    let tree = read_tree(store, at)?;

    /// Una región abierta, con lo que hace falta para poder seguir abriéndola.
    struct Abierta {
        id: EntityId,
        zoom: Zoom,
        /// Cuántas veces se bajó para llegar aquí. Las raíces están a cero.
        nivel: u32,
        /// Hijas ya promovidas desde aquí. Sólo pasa de cero en `Remainder`,
        /// que es literalmente «lo que queda de esta carpeta».
        tomadas: usize,
    }

    // Hijas por peso. Se pide varias veces y el orden tiene que ser idéntico
    // siempre: es de donde sale que dos ejecuciones abran el mismo cielo.
    let hijas_de = |h: &EntityId| -> Vec<EntityId> {
        let mut v: Vec<EntityId> = tree
            .children
            .get(h)
            .into_iter()
            .flatten()
            .filter(|c| !tree.muda(c))
            .cloned()
            .collect();
        v.sort_by(|a, b| tree.weight.get(b).cmp(&tree.weight.get(a)).then_with(|| a.cmp(b)));
        v
    };

    // Lo que se queda en la madre pase lo que pase: todo lo que cuelga de una
    // hija muda. Sin sumarlo, el resto pesaría menos de lo que cubre y dejaría
    // de competir antes de tiempo — el mismo fallo que el test de los 100
    // archivos cazó con las hijas no promovidas.
    let peso_mudo = |h: &EntityId| -> usize {
        tree.children
            .get(h)
            .into_iter()
            .flatten()
            .filter(|c| tree.muda(c))
            .map(|c| tree.weight.get(c).copied().unwrap_or(0))
            .sum()
    };

    let mut open: Vec<Abierta> = tree
        .folders
        .keys()
        .filter(|f| !tree.parent.contains_key(*f))
        .cloned()
        .map(|id| Abierta { id, zoom: Zoom::Subtree, nivel: 0, tomadas: 0 })
        .collect();
    open.sort_by(|a, b| a.id.cmp(&b.id));

    let peso = |a: &Abierta| -> usize {
        match a.zoom {
            Zoom::Subtree => tree.weight.get(&a.id).copied().unwrap_or(0),
            // Un resto pesa lo suyo **más lo que todavía no ha soltado**. Sin
            // esa segunda mitad, las hijas que no cupieron en el primer paso
            // quedarían enterradas para siempre y el cielo se quedaría corto:
            // pasó, y el test de los 100 archivos lo cazó — 4 regiones donde
            // tenían que salir 10.
            Zoom::Remainder => {
                tree.direct.get(&a.id).map_or(0, Vec::len)
                    + peso_mudo(&a.id)
                    + hijas_de(&a.id)
                        .iter()
                        .skip(a.tomadas)
                        .map(|c| tree.weight.get(c).copied().unwrap_or(0))
                        .sum::<usize>()
            }
        }
    };

    let cuantas = effective(target, tree.files);
    // Techo de hijas por paso. √regiones: la misma curva que `effective` usa
    // para el total, y por el mismo motivo — crece, pero cada vez menos.
    let tope = ((cuantas as f64).sqrt().ceil() as usize).max(2);
    while open.len() < cuantas {
        // La mayor que todavía se pueda abrir, **con rendimientos
        // decrecientes al bajar**.
        //
        // Elegir por peso a secas parte una y otra vez la misma rama, y un
        // árbol de código gana ese concurso siempre: es hondo, ancho y
        // uniforme. El primer corpus real (2026-08-10) lo enseñó sin lugar a
        // dudas — un único `whisper.cpp` de terceros se llevó **21 de las 30
        // regiones**, mientras `OneDrive`, donde estaba la vida entera del
        // usuario, se quedaba en un solo grumo sin abrir.
        //
        // Así que el peso se divide por el nivel: bajar otra vez por donde ya
        // bajaste exige el doble de peso, luego el triple. Una rama honda
        // sigue pudiendo abrirse mucho —si de verdad pesa— pero deja de
        // hacerlo *gratis*, y el cielo vuelve a enseñar la anchura de una vida
        // antes que el detalle de un rincón.
        //
        // Se compara en cruz en vez de dividir: exacto, sin redondeo, y sin
        // que el orden dependa de en qué punto se pierde la precisión.
        let Some(i) = open
            .iter()
            .enumerate()
            .filter(|(_, a)| !tree.atomica(&a.id) && hijas_de(&a.id).len() > a.tomadas)
            .max_by(|(_, a), (_, b)| {
                let ia = peso(a) as u64 * (1 + b.nivel as u64);
                let ib = peso(b) as u64 * (1 + a.nivel as u64);
                ia.cmp(&ib).then_with(|| b.id.cmp(&a.id))
            })
            .map(|(i, _)| i)
        else {
            break;
        };

        let madre = open.remove(i);
        let hijas = hijas_de(&madre.id);
        let pendientes = hijas.len() - madre.tomadas;

        // **No se promueven todas.** Una carpeta de fotos con doscientos años
        // dentro convertiría un solo paso en doscientas estrellas. Se ascienden
        // las mayores hasta llenar el techo y el resto se queda agregado bajo la
        // madre, que es lo que hace un mapa al alejarse: unas cuantas ciudades
        // con nombre y el campo alrededor.
        // El techo era «las que quepan», y al principio caben casi todas: por
        // ahí se coló que un único repositorio de terceros se llevara 19 de las
        // 30 regiones **en un solo paso**. El comentario de arriba describía la
        // intención correcta y la implementación no la cumplía.
        //
        // El tope por paso es √regiones — la misma curva que `effective` usa
        // para el total, y por el mismo motivo: crece, pero cada vez menos. Una
        // carpeta con veinte hijas aporta un puñado de estrellas con nombre y
        // el resto queda agregado bajo la madre, esperando su turno. Si de
        // verdad pesa, lo gana en la siguiente vuelta; pero lo gana compitiendo,
        // no por llegar antes.
        let sitio = cuantas.saturating_sub(open.len());
        let margen = sitio.min(tope);
        let todas = pendientes <= margen;
        let promovidas = if todas { pendientes } else { margen.saturating_sub(1) };
        if promovidas == 0 {
            open.push(madre);       // no cabe ni una más: el reparto terminó
            break;
        }

        for c in hijas.iter().skip(madre.tomadas).take(promovidas) {
            open.push(Abierta {
                id: c.clone(),
                zoom: Zoom::Subtree,
                nivel: madre.nivel + 1,
                tomadas: 0,
            });
        }
        // Lo que no subió —archivos propios de la madre y las hijas que aún no
        // han salido— sigue existiendo y no puede quedarse sin sitio. Y a
        // diferencia de antes, **puede volver a competir**: el resto sigue
        // siendo abrible mientras le queden hijas dentro.
        let tomadas = madre.tomadas + promovidas;
        let queda = tomadas < hijas.len()
            || tree.direct.get(&madre.id).is_some_and(|d| !d.is_empty())
            || peso_mudo(&madre.id) > 0;
        if queda {
            open.push(Abierta {
                id: madre.id,
                zoom: Zoom::Remainder,
                nivel: madre.nivel,
                tomadas,
            });
        }
    }

    // ── a qué región pertenece cada archivo ──
    //
    // El reparto va **antes** de construir las estrellas, y el peso de cada una
    // sale de contar lo que le tocó. Es la única forma de que no puedan
    // discrepar: con dos cálculos —uno para el tamaño y otro para el reparto—
    // llegaría el día en que una estrella se dibuja grande y al abrirla no hay
    // nada dentro.
    let abiertas: HashSet<EntityId> = open.iter().map(|a| a.id.clone()).collect();
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
    for Abierta { id: h, zoom: z, .. } in &open {
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

// ────────────────────────────────────── el mismo árbol, bajado un escalón
//
// [ADR-0014] pide dos vistas sobre un solo estado, y da por hecho que el núcleo
// ya las servía las dos. Casi: `regions` corta el árbol alto y `cluster` agrupa
// **resultados de búsqueda**, que no es lo mismo que *«qué hay dentro de esto»*.
// Sin esa tercera lectura no hay forma de bajar un nivel, y bajar un nivel es el
// hito entero.
//
// Se apoya en el mismo [`read_tree`] que abre el cielo, y eso no es ahorro de
// código: es la garantía de la consecuencia #4 del ADR. Con dos lecturas
// distintas del árbol llegaría el día en que una estrella dice «229 archivos» y
// al entrar hay 217, y en ese momento dejan de ser dos vistas del mismo estado.
//
// [ADR-0014]: ../../../docs/adr/0014-dos-vistas-un-solo-estado.md

/// Relaciones que significan **«esto está dentro de aquello»**.
///
/// El ADR exige que el explorador navegue el grafo y no el disco: *«se baja por
/// carpetas, y también por entidad → dónde aparece»*. Ese segundo eje nombra la
/// mención de una entidad, que la crea M6 y hoy no existe. Lo que sí existe
/// desde M3 es la **sesión de trabajo**: 961 aserciones que dicen qué tocaste a
/// la vez. Es contención de verdad —agrupa por *cuándo*, no por *dónde*— y es lo
/// que hace que esto no sea el explorador de Windows con pasos de más.
pub const CONTAINMENT: &[&str] = &["contained_by", "occurred_during"];

/// Un escalón de las migas de pan.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Crumb {
    pub id: String,
    pub label: String,
}

/// Una fila del explorador.
///
/// Trae ya resueltos los cuatro datos que la fila necesita para **decir la
/// verdad sin volver a preguntar**: si se puede leer su contenido, cuánto pesa,
/// qué retiene la puerta y cuándo se tocó. En particular [`Row::availability`],
/// que es la señal que impide que una previsualización hidrate un archivo de la
/// nube por accidente: la decide M1 y no se recalcula aquí.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Row {
    pub id: String,
    pub label: String,
    pub path: String,
    /// `folder` | `file`.
    pub kind: &'static str,
    /// Archivos dentro, contando subcarpetas. `0` en un archivo.
    pub weight: usize,
    pub sensitivity: &'static str,
    /// `local` | `cloud_dehydrated`, o `None` si no se sabe.
    ///
    /// `None` es deliberadamente distinto de `local`: [`Availability::parse`]
    /// devuelve `None` ante un valor desconocido *a propósito*, y ante la duda no
    /// se toca el archivo. Un `unwrap_or(local)` en el cliente convertiría esa
    /// prudencia en una descarga.
    pub availability: Option<String>,
    pub recency: i64,
    pub facets: Vec<String>,
}

/// Lo que hay dentro de un contenedor, con la ruta hasta él.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Listing {
    /// `None` en la raíz.
    pub focus: Option<String>,
    pub label: String,
    pub path: String,
    /// De fuera hacia dentro. Es la ruta que el conmutador tiene que traer ya
    /// puesta al venir de la galaxia (consecuencia #4 del ADR).
    pub trail: Vec<Crumb>,
    pub rows: Vec<Row>,
    pub folders: usize,
    pub files: usize,
}

/// Lee un predicado entero y lo indexa por sujeto. Una consulta por predicado y
/// no una por fila: es el mismo patrón que usan [`regions`] y
/// `obrain_link::load_files`, y la diferencia entre 4 consultas y 900.
fn by_subject(store: &Store, pred: &str, at: TimePoint) -> Result<HashMap<EntityId, Value>> {
    let mut out = HashMap::new();
    for a in store.query(None, Some(pred), at)? {
        out.entry(a.subject).or_insert(a.object);
    }
    Ok(out)
}

/// **Qué hay dentro de esto.** El tercer corte del árbol de contención.
///
/// Con `focus` a `None` devuelve las raíces, que es por donde se entra cuando
/// nadie ha enfocado nada todavía.
pub fn children(store: &Store, focus: Option<&EntityId>, at: TimePoint) -> Result<Listing> {
    let tree = read_tree(store, at)?;
    let modificado = by_subject(store, "modified_at", at)?;
    let ruta = by_subject(store, "path_observed_at", at)?;
    let disponible = by_subject(store, "availability", at)?;
    let nombres = obrain_link::Labels::load(store, at)?;

    // Una faceta por fila sería otra consulta por fila. `has_facet` es el
    // predicado más numeroso del log (1.919 sobre este corpus) y aun así leerlo
    // entero una vez cuesta menos que preguntarlo doscientas.
    let mut facetas: HashMap<EntityId, Vec<String>> = HashMap::new();
    for a in store.query(None, Some("has_facet"), at)? {
        if let Some(t) = a.object.as_text() {
            facetas.entry(a.subject).or_default().push(t.to_string());
        }
    }

    // La sensibilidad no es una aserción sino una columna (ADR-0006), así que
    // se lee de la tabla y de una vez, como hace `audit` un poco más arriba.
    let mut sensible: HashMap<String, &'static str> = HashMap::new();
    {
        let conn = store.conn();
        let mut st = conn.prepare("SELECT id, sensitivity FROM entities")?;
        let mut rows = st.query([])?;
        while let Some(r) = rows.next()? {
            let nivel: String = r.get(1)?;
            sensible.insert(
                r.get::<_, String>(0)?,
                obrain_core::Sensitivity::parse(&nivel).map_or("vault", |s| s.as_str()),
            );
        }
    }

    let stamp = |e: &EntityId| -> i64 {
        match modificado.get(e) {
            Some(Value::Timestamp(t)) => *t,
            _ => 0,
        }
    };
    let sitio = |e: &EntityId| -> String {
        tree.folders.get(e).cloned().unwrap_or_else(|| match ruta.get(e) {
            Some(Value::Text(p)) => p.clone(),
            _ => String::new(),
        })
    };

    let fila = |e: &EntityId, carpeta: bool| -> Row {
        Row {
            // Sin nombre por ruta ni por carpeta sólo quedan las sesiones, que
            // no aparecen como filas. Se paga la consulta suelta antes que
            // enseñar un identificador.
            label: nombres
                .get(e)
                .map_or_else(|| obrain_link::label(store, e, at), str::to_string),
            path: sitio(e),
            kind: if carpeta { "folder" } else { "file" },
            weight: if carpeta {
                tree.weight.get(e).copied().unwrap_or(0)
            } else {
                0
            },
            sensitivity: sensible.get(&e.0).copied().unwrap_or("vault"),
            availability: match disponible.get(e) {
                Some(Value::Text(t)) => Some(t.clone()),
                _ => None,
            },
            recency: stamp(e),
            facets: facetas.get(e).cloned().unwrap_or_default(),
            id: e.0.clone(),
        }
    };

    // ── qué cuelga de aquí ──
    let (carpetas, archivos): (Vec<EntityId>, Vec<EntityId>) = match focus {
        // La raíz: las carpetas sin madre. No hay ninguna entidad que las
        // contenga, así que no hay nada que preguntar al log.
        None => (
            {
                let mut v: Vec<EntityId> = tree
                    .folders
                    .keys()
                    .filter(|f| !tree.parent.contains_key(*f))
                    .cloned()
                    .collect();
                v.sort();
                v
            },
            Vec::new(),
        ),
        Some(f) if tree.folders.contains_key(f) => (
            tree.children.get(f).cloned().unwrap_or_default(),
            tree.direct.get(f).cloned().unwrap_or_default(),
        ),
        // No es una carpeta: entonces es cualquier otra cosa que contenga, y lo
        // que contiene es lo que la señala. Aquí es donde entran las sesiones —
        // y donde entrarán las menciones de M6 sin tocar esta función.
        Some(f) => {
            let mut v: Vec<EntityId> = store
                .query_by_object(f, at)?
                .into_iter()
                .filter(|a| CONTAINMENT.contains(&a.predicate.as_str()))
                .map(|a| a.subject)
                .collect();
            v.sort();
            v.dedup();
            (Vec::new(), v)
        }
    };

    // Las carpetas primero y por peso: lo que más tiene dentro es lo que más
    // probablemente busca quien está bajando. Los archivos, por lo más reciente
    // —que es como se busca un papel de verdad— y con el nombre de desempate
    // para que dos ejecuciones den siempre el mismo orden.
    let mut rows: Vec<Row> = Vec::with_capacity(carpetas.len() + archivos.len());
    let mut cs: Vec<Row> = carpetas.iter().map(|c| fila(c, true)).collect();
    cs.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.label.cmp(&b.label)));
    let mut fs: Vec<Row> = archivos.iter().map(|a| fila(a, false)).collect();
    fs.sort_by(|a, b| b.recency.cmp(&a.recency).then_with(|| a.label.cmp(&b.label)));
    let (folders, files) = (cs.len(), fs.len());
    rows.append(&mut cs);
    rows.append(&mut fs);

    // ── las migas ── de fuera hacia dentro, con tope: una arista de contención
    // mal formada no puede convertir esto en un bucle infinito.
    let mut trail = Vec::new();
    if let Some(f) = focus {
        let mut cur = f.clone();
        let mut vistos = HashSet::new();
        while vistos.insert(cur.clone()) && trail.len() < 32 {
            let Some(up) = tree.parent.get(&cur).cloned() else { break };
            trail.push(Crumb {
                label: nombres
                    .get(&up)
                    .map_or_else(|| obrain_link::label(store, &up, at), str::to_string),
                id: up.0.clone(),
            });
            cur = up;
        }
        trail.reverse();
    }

    Ok(Listing {
        label: focus.map_or_else(|| "todo".to_string(), |f| obrain_link::label(store, f, at)),
        path: focus.map_or_else(String::new, sitio),
        focus: focus.map(|f| f.0.clone()),
        trail,
        rows,
        folders,
        files,
    })
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
