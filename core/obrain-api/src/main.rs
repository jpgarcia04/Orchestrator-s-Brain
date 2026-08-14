//! `--sample` para el mapa.
//!
//! M3 dejó escrita la conclusión metodológica del proyecto: *«un enlazador que
//! no se puede inspeccionar es un enlazador en el que no se puede confiar»*, y
//! por eso `--sample` pasó a ser parte de aquel hito en vez de una herramienta
//! de depuración. Una lente que no se puede inspeccionar tiene el mismo
//! problema, y uno peor: cuando lo que falla es lo que se dibuja, el fallo se
//! confunde con una decisión de diseño.
//!
//! Esto imprime, sin abrir ninguna ventana, lo que el mapa va a enseñar.

use obrain_api::{lens, sky, Brain};
use obrain_core::{EntityId, TimePoint};
use std::process::ExitCode;
use std::time::Instant;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!(
            "obrain-api — M5a\n\n\
             USO: obrain-api <base-de-datos> [consulta] [opciones]\n\n\
             Sin consulta imprime el censo del grafo: quien va a salir grande.\n\
             Con consulta imprime la lista de resultados y la lente que abren.\n\n\
             OPCIONES:\n  \
               --cielo            el grafo entero a poca escala: las regiones\n  \
               --regiones <n>     cuantas regiones abrir (por defecto 30)\n  \
               --radio <n>        saltos desde el foco (por defecto 1 con consulta, 2 con --foco)\n  \
               --presupuesto <n>  tope duro de nodos (por defecto 300)\n  \
               --foco <id>        abre la lente sobre una entidad concreta\n  \
               --modelo           carga el modelo y enciende la rama semantica\n  \
               --explorar         que hay dentro: el explorador de ADR-0014\n  \
               --dentro <id>      baja a esta entidad (sin el, las raices)\n  \
               --ver <id>         que se ve del archivo: la previsualizacion de M5b\n  \
               --limite <n>       filas a enseñar (por defecto 40)\n"
        );
        return ExitCode::from(2);
    }

    let db = &args[0];
    let flag = |n: &str| args.iter().any(|a| a == n);
    let value = |n: &str| {
        args.iter()
            .position(|a| a == n)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let numero = |n: &str, por_defecto: usize| {
        value(n).and_then(|v| v.parse().ok()).unwrap_or(por_defecto)
    };
    // La consulta es el primer argumento suelto que no sea una opción ni el
    // valor de una.
    let consulta = args[1..]
        .iter()
        .enumerate()
        .find(|(i, a)| {
            !a.starts_with("--")
                && !args[1..]
                    .get(i.wrapping_sub(1))
                    .is_some_and(|p| p.starts_with("--"))
        })
        .map(|(_, a)| a.clone());

    let mut brain = match Brain::open(db) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("no se pudo abrir {db}: {e}");
            return ExitCode::FAILURE;
        }
    };

    if flag("--modelo") {
        let t = Instant::now();
        match brain.warm(None) {
            Ok(true) => println!("  modelo cargado en {:.2} s", t.elapsed().as_secs_f64()),
            Ok(false) => println!("  sin modelo local — la busqueda va en lexico"),
            Err(e) => println!("  no se pudo cargar el modelo: {e}"),
        }
    }

    let presupuesto = numero("--presupuesto", lens::BUDGET);
    let at = TimePoint::at(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0),
    );

    // ── auditar el dimensionado: qué señal ordena bien las regiones ──
    //
    // Como `--sample` en M2, M3 y M4: no es una herramienta de depuración, es
    // la parte del trabajo donde se mira si el contador dice la verdad.
    if flag("--pesos") {
        let objetivo = numero("--regiones", sky::REGIONS);
        let at = obrain_core::TimePoint::at(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
        );
        match sky::audit(brain.store(), objetivo, at) {
            Ok(a) => {
                type Señal = (&'static str, fn(&sky::RegionAudit) -> i64);
                let señales: [Señal; 6] = [
                    ("archivos   (regla de hoy)", |r| r.files as i64),
                    ("aserciones (cuánto sabe)", |r| r.assertions as i64),
                    ("sesiones   (cuántas veces volviste)", |r| r.sessions as i64),
                    ("lapso/días (cuánta vida abarca)", |r| r.span_days),
                    ("aristas    (con qué se relaciona)", |r| r.out_edges as i64),
                    // Derivada: no cuánto hay, sino cuánto sabe el sistema de
                    // cada cosa. Un estado de cuenta extraído y vectorizado
                    // deja muchas más aserciones que una captura de pantalla.
                    ("aserciones POR archivo (riqueza)", |r| {
                        r.assertions.checked_div(r.files).unwrap_or(0) as i64
                    }),
                ];
                for (titulo, f) in señales {
                    let mut v: Vec<&sky::RegionAudit> = a.iter().collect();
                    v.sort_by_key(|r| -f(r));
                    println!("\n── por {titulo} ──");
                    for r in v.iter().take(8) {
                        println!("  {:>7}  {}", f(r), r.label);
                    }
                }
            }
            Err(e) => eprintln!("pesos: {e}"),
        }
        return ExitCode::SUCCESS;
    }

    // ── el cielo: el grafo entero a poca escala ──
    if flag("--cielo") {
        let objetivo = numero("--regiones", sky::REGIONS);
        let t = Instant::now();
        match brain.sky(objetivo) {
            Ok(c) if flag("--json") => {
                println!("{}", serde_json::to_string(&c).unwrap_or_default());
            }
            Ok(c) => {
                println!(
                    "\n── EL CIELO ── {} regiones · {}/{} archivos cubiertos · {:.1} ms ──",
                    c.regions.len(),
                    c.covered,
                    c.files,
                    t.elapsed().as_secs_f64() * 1000.0
                );
                for r in &c.regions {
                    let marca = if r.sensitivity == "normal" { "  " } else { " ⚠" };
                    let resto = if r.remainder { " (resto)" } else { "" };
                    println!("  {:>5}{marca} d{}  {}{resto}", r.weight, r.depth, r.label);
                }
                if !c.bridges.is_empty() {
                    println!(
                        "\n  puentes — sesiones que cruzaron dos regiones ({} de {}):",
                        c.bridges.len(),
                        c.bridges_found
                    );
                    let por_id: std::collections::HashMap<&str, &str> = c
                        .regions
                        .iter()
                        .map(|r| (r.id.as_str(), r.label.as_str()))
                        .collect();
                    for b in c.bridges.iter().take(10) {
                        println!(
                            "    {:>3}  {}  ↔  {}",
                            b.weight,
                            por_id.get(b.from.as_str()).unwrap_or(&"?"),
                            por_id.get(b.to.as_str()).unwrap_or(&"?")
                        );
                    }
                }
            }
            Err(e) => eprintln!("cielo: {e}"),
        }
        return ExitCode::SUCCESS;
    }

    // ── el explorador: qué hay dentro de esto ──
    //
    // Sin `--dentro` entra por las raíces, que es por donde entra el explorador
    // cuando nadie ha enfocado nada. Con él, se baja: es el `--sample` de
    // ADR-0014, y existe por lo mismo que los otros — un explorador que enseña
    // mal se confunde con una decisión de diseño si no se puede mirar el dato
    // que lo produjo.
    if flag("--explorar") {
        let foco = value("--dentro").map(EntityId);
        let t = Instant::now();
        match brain.children(foco.as_ref()) {
            Ok(l) if flag("--json") => {
                println!("{}", serde_json::to_string(&l).unwrap_or_default());
            }
            Ok(l) => {
                let migas: Vec<&str> = l.trail.iter().map(|c| c.label.as_str()).collect();
                println!(
                    "\n── DENTRO DE «{}» ── {} carpetas · {} archivos · {:.1} ms ──",
                    l.label,
                    l.folders,
                    l.files,
                    t.elapsed().as_secs_f64() * 1000.0
                );
                if !migas.is_empty() {
                    println!("  {}  ›  {}", migas.join("  ›  "), l.label);
                }
                println!();
                for r in l.rows.iter().take(numero("--limite", 40)) {
                    let marca = if r.sensitivity == "normal" { "  " } else { " ⚠" };
                    // Lo que no se puede leer se dice **en la fila**, no en un
                    // panel al final: es la señal que decide si algo se puede
                    // previsualizar sin descargarlo.
                    let nube = match r.availability.as_deref() {
                        Some("local") | None => "",
                        Some(_) => "  ☁ en la nube",
                    };
                    let peso = if r.kind == "folder" {
                        format!("{:>6}", r.weight)
                    } else {
                        "      ".to_string()
                    };
                    let icono = if r.kind == "folder" { "▸" } else { " " };
                    println!("  {icono} {peso}{marca}  {}{nube}", r.label);
                }
                if l.rows.len() > numero("--limite", 40) {
                    println!("  … y {} más", l.rows.len() - numero("--limite", 40));
                }
                println!("\n  id del foco: {}", l.focus.as_deref().unwrap_or("(raíces)"));
            }
            Err(e) => eprintln!("explorar: {e}"),
        }
        return ExitCode::SUCCESS;
    }

    // ── la previsualización: qué se ve del archivo ──
    //
    // El `--sample` de M5b, y hace falta más que los otros: lo que este hito
    // entrega **es** lo que se le enseña a una persona, y el informe del corpus
    // real ya dejó escrita la lección —se auditaron los contadores, se auditó
    // el contenido, y lo que nunca se había auditado era lo que aparece en
    // pantalla, que es una tercera cosa distinta de las dos—. Esto imprime,
    // sin abrir ninguna ventana, exactamente lo que va a leer el panel.
    if let Some(id) = value("--ver") {
        let e = EntityId(id);
        let t = Instant::now();
        match brain.preview(&e, consulta.as_deref()) {
            Ok(v) if flag("--json") => {
                println!("{}", serde_json::to_string(&v).unwrap_or_default());
            }
            Ok(v) => {
                println!("\n── QUÉ SE VE ── {:.1} ms ──", t.elapsed().as_secs_f64() * 1000.0);
                println!("  ruta            {}", v.path.as_deref().unwrap_or("(sin ruta)"));
                println!(
                    "  disponibilidad  {}  →  {}",
                    v.availability.as_deref().unwrap_or("(sin etiqueta)"),
                    if v.readable { "se puede abrir" } else { "NO SE ABRE" }
                );
                println!("  sensibilidad    {}", v.sensitivity);
                println!("  qué hay         {}", v.kind);
                match v.kind {
                    "text" => {
                        println!(
                            "  texto           {} caracteres · ventana desde el byte {}{}",
                            v.chars,
                            v.offset,
                            if v.truncated { " · CORTADO" } else { "" }
                        );
                        if let Some((s, f)) = v.highlight {
                            println!("  marcado         [{s}, {f}) dentro de la ventana");
                        }
                        // Las primeras líneas, que es lo que se ve sin
                        // desplazar el panel.
                        if let Some(t) = &v.text {
                            println!("\n  ── lo que se lee al abrirlo ──");
                            for linea in t.lines().filter(|l| !l.trim().is_empty()).take(6) {
                                println!("  │ {}", linea.chars().take(100).collect::<String>());
                            }
                        }
                    }
                    "image" => println!(
                        "  miniatura       {}×{} px · {} KB en base64",
                        v.width,
                        v.height,
                        v.image.as_ref().map(|s| s.len() / 1024).unwrap_or(0)
                    ),
                    _ => println!("  por qué no      {}", v.reason.as_deref().unwrap_or("—")),
                }
            }
            Err(e) => eprintln!("ver: {e}"),
        }
        return ExitCode::SUCCESS;
    }

    // ── el censo: lo que hay antes de mirar nada ──
    println!("\n── EL GRAFO ───────────────────────────────");
    match lens::edge_census(brain.store(), at) {
        Ok(censo) => {
            let total: usize = censo.iter().map(|(_, n)| n).sum();
            for (p, n) in &censo {
                println!("  {p:<18} {n:>6}");
            }
            println!("  {:<18} {total:>6}", "ARISTAS VIVAS");
        }
        Err(e) => eprintln!("  censo: {e}"),
    }

    println!("\n── QUIEN VA A SALIR GRANDE ────────────────");
    match lens::hubs(brain.store(), 8, at) {
        Ok(hubs) => {
            for (etiqueta, grado) in hubs {
                println!("  {grado:>4}  {etiqueta}");
            }
        }
        Err(e) => eprintln!("  hubs: {e}"),
    }

    // ── la lente ──
    let lente = if let Some(id) = value("--foco") {
        let radio = numero("--radio", 2) as u8;
        println!("\n── LENTE SOBRE {id} (radio {radio}) ──");
        brain.lens(&[EntityId(id)], radio, presupuesto)
    } else if let Some(q) = consulta {
        let t = Instant::now();
        let vistazo = match brain.glance(&q, 10) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("consulta: {e}");
                return ExitCode::FAILURE;
            }
        };
        println!(
            "\n── «{q}» ─── {} resultados en {:.1} ms · {} ───",
            vistazo.answer.hits.len(),
            t.elapsed().as_secs_f64() * 1000.0,
            vistazo.answer.branches.join(" + ")
        );
        for (i, h) in vistazo.answer.hits.iter().enumerate() {
            let marca = if h.sensitivity == "normal" { "" } else { " ⚠" };
            let ramas: Vec<String> =
                h.ranks.iter().map(|(r, p)| format!("{r}#{p}")).collect();
            println!("  {:>2}. {}{marca}   [{}]", i + 1, h.label, ramas.join(" "));
            if !h.snippet.is_empty() {
                println!("      {}", h.snippet.chars().take(96).collect::<String>());
            }
        }
        Ok(vistazo.lens)
    } else {
        println!("\n  (sin consulta: sólo el censo)");
        return ExitCode::SUCCESS;
    };

    match lente {
        // La lente era la única superficie sin `--json`, y es la que dibuja el
        // mapa. Sin esto no hay forma de mirar el dato que produjo un fotograma
        // —que es la convención de este proyecto y ya ha desmentido dos
        // diagnósticos escritos—: había que abrir la aplicación y creerse lo
        // que se veía.
        Ok(l) if flag("--json") => {
            println!("{}", serde_json::to_string(&l).unwrap_or_default());
        }
        Ok(l) => {
            println!("\n── LA LENTE ───────────────────────────────");
            println!("  focos                 {}", l.focus.len());
            println!("  nodos dibujados       {}", l.nodes.len());
            println!("  aristas dibujadas     {}", l.edges.len());
            println!("  presupuesto           {}", l.budget);
            println!(
                "  alcanzados            {}{}",
                l.reached,
                if l.truncated { "  ← CORTADO" } else { "" }
            );
            let punteadas = l.edges.iter().filter(|e| e.method == "inferred").count();
            println!(
                "  trazo                 {} solidas · {punteadas} punteadas",
                l.edges.len() - punteadas
            );
            let boveda = l.nodes.iter().filter(|n| n.sensitivity != "normal").count();
            if boveda > 0 {
                println!("  marcados por la puerta {boveda}");
            }

            println!("\n  nodos por distancia al foco:");
            for d in 0..=4u8 {
                let n = l.nodes.iter().filter(|x| x.distance == d).count();
                if n > 0 {
                    println!("    {d}  {n:>4}");
                }
            }

            println!("\n  los mas grandes de la lente:");
            let mut orden: Vec<_> = l.nodes.iter().collect();
            orden.sort_by_key(|n| std::cmp::Reverse(n.degree));
            for n in orden.iter().take(6) {
                println!("    {:>4}  d{}  {}", n.degree, n.distance, n.label);
            }
        }
        Err(e) => eprintln!("lente: {e}"),
    }

    ExitCode::SUCCESS
}
