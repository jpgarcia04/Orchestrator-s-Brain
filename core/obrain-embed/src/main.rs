//! CLI de vectorización y búsqueda semántica. Se ejecuta sobre una base ya
//! escaneada, enlazada, extraída y clasificada.
//!
//! `--sample` vuelve a ser parte del hito, por quinta vez y con un motivo
//! nuevo. En M1, M2, M3 y en la puerta los defectos reales no aparecieron en
//! los contadores. En un espacio vectorial es peor: la patología clásica —unos
//! pocos fragmentos que salen como vecinos plausibles de casi cualquier cosa,
//! los *hubs*— **no tiene ningún contador que la delate**. El índice está
//! completo, la cobertura es del 100 %, la aritmética cierra, y la búsqueda es
//! mala. Sólo se ve mirando vecinos de verdad.

use obrain_core::{Store, TimePoint};
use obrain_embed::embedder::{Embedder, cosine};
use obrain_embed::{EmbedReport, download, e5, index, search};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!(
            "obrain-embed — M4 · Entender\n\n\
             USO: obrain-embed <base-de-datos> [opciones]\n\n\
             OPCIONES:\n  \
               --buscar <texto>    buscar: BM25, kNN y su fusion RRF, lado a lado\n  \
               --limite <n>        resultados a enseñar (por defecto 8)\n  \
               --sample            auditar el espacio vectorial: vecinos, hubs, comparativa\n  \
               --medir             presupuesto de latencia del kNN y del rerank\n  \
               --dry-run           no escribir vectores\n  \
               --modelo <dir>      directorio del modelo\n  \
               --descargar-modelo  bajar el modelo (LA UNICA vez que se toca la red)\n"
        );
        return ExitCode::from(2);
    }

    let flag = |n: &str| args.iter().any(|a| a == n);
    let value = |n: &str| {
        args.iter()
            .position(|a| a == n)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };

    let model_dir: PathBuf = value("--modelo")
        .map(PathBuf::from)
        .unwrap_or_else(download::default_dir);

    // ── el paso deliberado de red ──
    if flag("--descargar-modelo") {
        println!("\n▸ modelo {} → {}", e5::MODEL.label(), model_dir.display());
        println!("  origen: {}\n", download::BASE);
        match download::fetch(&model_dir) {
            Ok(bytes) => {
                println!("\n  {:.1} MB en disco", bytes as f64 / 1_048_576.0);
                println!("  A partir de aqui el pipeline funciona sin conexion.\n");
                return ExitCode::SUCCESS;
            }
            Err(e) => {
                eprintln!(
                    "\n  fallo: {e}\n\n  Para bajarlo a mano:\n{}",
                    download::manual_instructions(&model_dir)
                );
                return ExitCode::FAILURE;
            }
        }
    }

    let mut store = match Store::open(&args[0]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("no se pudo abrir {}: {e}", args[0]);
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = index::migrate(store.conn()) {
        eprintln!("no se pudo crear la proyeccion vectorial: {e}");
        return ExitCode::FAILURE;
    }

    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let tp = TimePoint::at(at);
    let limit: usize = value("--limite").and_then(|v| v.parse().ok()).unwrap_or(8);

    if !download::is_present(&model_dir) {
        eprintln!("\n  No hay modelo en {}\n", model_dir.display());
        eprintln!(
            "  Ejecuta una vez:  obrain-embed {} --descargar-modelo\n",
            args[0]
        );
        eprintln!(
            "  O bajalo a mano:\n{}",
            download::manual_instructions(&model_dir)
        );
        return ExitCode::FAILURE;
    }

    let t_load = std::time::Instant::now();
    let embedder = match e5::E5::load(&model_dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("no se pudo cargar el modelo: {e}");
            return ExitCode::FAILURE;
        }
    };
    let model_load = t_load.elapsed();

    // ── buscar ──
    if let Some(q) = value("--buscar") {
        return buscar(&store, &embedder, &q, limit, tp, model_load);
    }

    // ── vectorizar ──
    let mut report = EmbedReport {
        model_load,
        ..Default::default()
    };
    if !flag("--dry-run") {
        println!("\n▸ vectorizando con {}", embedder.model().label());
        println!("  modelo cargado en {:.2} s", model_load.as_secs_f64());
        let before = store.log_len().unwrap_or(0);
        let res = store.transaction(|s| obrain_embed::embed_all(s, &embedder, at, &mut report));
        if let Err(e) = res {
            eprintln!("error al vectorizar: {e}");
            return ExitCode::FAILURE;
        }
        let after = store.log_len().unwrap_or(0);
        informe(&store, &report, &embedder, before, after);
    }

    if flag("--medir") {
        medir(&store, &embedder);
    }
    if flag("--sample") {
        auditar(&store, &embedder, tp);
    }
    println!();
    ExitCode::SUCCESS
}

// ══════════════════════════════════════════════════════════════════ informe

fn informe(store: &Store, r: &EmbedReport, embedder: &dyn Embedder, before: i64, after: i64) {
    let model = embedder.model();
    println!("\n── LA PUERTA (I11) ────────────────────────");
    println!("  entidades concedidas    {}", r.entities_allowed);
    println!("  entidades retenidas     {}", r.entities_withheld);
    println!("  fragmentos retenidos    {}", r.chunks_withheld);
    println!(
        "  rechazos en la llamada  {}  (debe ser 0)",
        r.denied_at_call
    );

    println!("\n── INDICE VECTORIAL ───────────────────────");
    println!("  fragmentos pendientes   {}", r.pending);
    println!("  vectores escritos       {}", r.vectors_written);
    let total = index::count(store.conn(), model).unwrap_or(0);
    println!("  vectores en el indice   {total}");
    println!("  dimension               {}", model.dim);
    println!(
        "  bytes de vectores       {:.2} MB",
        (total as f64 * model.dim as f64 * 4.0) / 1_048_576.0
    );

    println!("\n── COSTE DEL MODELO ───────────────────────");
    println!(
        "  carga del modelo        {:.2} s",
        r.model_load.as_secs_f64()
    );
    println!(
        "  dentro del modelo       {:.2} s",
        r.inference.as_secs_f64()
    );
    println!("  total                   {:.2} s", r.elapsed.as_secs_f64());
    if r.vectors_written > 0 && r.inference.as_secs_f64() > 0.0 {
        println!(
            "  ritmo                   {:.0} fragmentos/s",
            r.vectors_written as f64 / r.inference.as_secs_f64()
        );
    }

    println!("\n── CENSO DE MODELOS (I7) ──────────────────");
    for (id, ver, dim, n) in index::census(store.conn()).unwrap_or_default() {
        println!("  {id}@{ver}  dim {dim}  {n} vectores");
    }

    println!("\n── LOG ────────────────────────────────────");
    println!("  aserciones antes        {before}");
    println!("  aserciones ahora        {after}");
    println!("  escritas                {}", r.assertions_written);
    println!("  sin cambio              {}", r.assertions_unchanged);

    // ── aritmética, como en los cuatro informes anteriores ──
    println!("\n── ARITMETICA ─────────────────────────────");
    let con_texto: i64 = store
        .conn()
        .query_row("SELECT COUNT(DISTINCT entity) FROM chunks", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);
    let frag: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))
        .unwrap_or(0);
    println!(
        "  {} concedidas + {} retenidas = {} entidades con texto  {}",
        r.entities_allowed,
        r.entities_withheld,
        con_texto,
        marca(r.entities_allowed as i64 + r.entities_withheld as i64 == con_texto)
    );
    println!(
        "  {} fragmentos - {} retenidos = {} vectorizables  {}",
        frag,
        r.chunks_withheld,
        frag - r.chunks_withheld,
        marca(frag - r.chunks_withheld == total)
    );
}

fn marca(ok: bool) -> &'static str {
    if ok { "✓" } else { "✗ NO CUADRA" }
}

// ══════════════════════════════════════════════════════════════════ buscar

fn buscar(
    store: &Store,
    embedder: &dyn Embedder,
    q: &str,
    limit: usize,
    tp: TimePoint,
    model_load: std::time::Duration,
) -> ExitCode {
    let t = std::time::Instant::now();
    let (fused, lexical, vectorial) = match search::hybrid(store, embedder, q, limit, tp) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error al buscar: {e}");
            return ExitCode::FAILURE;
        }
    };
    let elapsed = t.elapsed();

    println!("\n▸ «{q}»");
    println!(
        "  modelo cargado en {:.2} s · consulta en {:.1} ms",
        model_load.as_secs_f64(),
        elapsed.as_secs_f64() * 1000.0
    );

    println!("\n── BM25 (lexico, M2) ──────────────────────");
    if lexical.is_empty() {
        println!("  (nada: ninguna palabra de la consulta esta en el corpus)");
    }
    for (i, h) in lexical.iter().take(limit).enumerate() {
        println!("  {:>2}. {:>8.2}  {}", i + 1, h.score, corto(&h.path));
    }

    println!("\n── kNN vectorial (M4) ─────────────────────");
    if vectorial.is_empty() {
        println!("  (nada)");
    }
    for (i, h) in vectorial.iter().take(limit).enumerate() {
        println!("  {:>2}. {:>8.3}  {}", i + 1, h.score, corto(&h.path));
    }

    println!("\n── FUSION RRF (k = 60) ────────────────────");
    for (i, h) in fused.iter().enumerate() {
        let proc: Vec<String> = h.ranks.iter().map(|(n, r)| format!("{n}#{r}")).collect();
        println!("  {:>2}. {:>8.5}  {}", i + 1, h.score, corto(&h.path));
        println!("      procedencia: {}", proc.join(", "));
        if !h.snippet.is_empty() {
            println!("      «{}»", h.snippet);
        }
    }
    println!();
    ExitCode::SUCCESS
}

fn corto(path: &str) -> String {
    let mut p = path.to_string();
    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        let home_fwd = home.replace('\\', "/");
        let home_bwd = home.replace('/', "\\");
        if let Some(rest) = p
            .strip_prefix(&home_fwd)
            .or_else(|| p.strip_prefix(&home_bwd))
        {
            let trimmed = rest.trim_start_matches(['/', '\\']);
            p = format!("~/{trimmed}");
        }
    }
    if p.chars().count() > 92 {
        let tail: String = p.chars().skip(p.chars().count() - 89).collect();
        format!("...{tail}")
    } else {
        p
    }
}

// ══════════════════════════════════════════════════════════════════ medir

/// El presupuesto de latencia, con los números delante.
///
/// Dos preguntas que §7.1 y §6 dejan abiertas y que no se pueden contestar de
/// memoria: ¿hace falta un índice ANN?, y ¿cabe un cross-encoder en el camino?
fn medir(store: &Store, embedder: &dyn Embedder) {
    let model = embedder.model();
    let n = index::count(store.conn(), model).unwrap_or(0);
    println!("\n── PRESUPUESTO DE LATENCIA ────────────────");

    // ── 1 · el kNN por recorrido lineal ──
    let qv = match embedder.embed(&["algo cualquiera"], obrain_embed::EmbedRole::Query) {
        Ok(v) => v.into_iter().next().unwrap_or_default(),
        Err(e) => {
            println!("  no se pudo embeber la consulta de prueba: {e}");
            return;
        }
    };
    let mut best = f64::MAX;
    for _ in 0..20 {
        let t = std::time::Instant::now();
        let _ = index::knn(store.conn(), &qv, model, 50);
        best = best.min(t.elapsed().as_secs_f64() * 1000.0);
    }
    println!("  desde SQLite, {n} vectores    {best:.2} ms  (mejor de 20)");

    // La misma búsqueda sobre la matriz ya en memoria. La diferencia entre
    // las dos cifras es lo único que decide qué hay que arreglar cuando esto
    // deje de bastar: si el coste está en el producto escalar, hace falta un
    // índice aproximado; si está en sacar los BLOB de SQLite y decodificarlos,
    // un índice aproximado no arregla nada y lo que hace falta es no volver a
    // leerlos. No es lo mismo, y sin medirlo por separado no se distingue.
    let residente = index::load_all(store.conn(), model).unwrap_or_default();
    let mut best_mem = f64::MAX;
    for _ in 0..20 {
        let t = std::time::Instant::now();
        let mut top: Vec<(f32, usize)> = residente
            .iter()
            .enumerate()
            .map(|(i, (_, _, v))| (cosine(&qv, v), i))
            .collect();
        top.sort_by(|a, b| b.0.total_cmp(&a.0));
        top.truncate(50);
        std::hint::black_box(&top);
        best_mem = best_mem.min(t.elapsed().as_secs_f64() * 1000.0);
    }
    println!("  con la matriz residente       {best_mem:.2} ms  (mejor de 20)");
    if n > 0 {
        let per = best / n as f64;
        let per_mem = best_mem / n as f64;
        println!("  coste por vector              {per:.5} ms  ·  residente {per_mem:.5} ms");
        println!(
            "  a 50 ms de presupuesto        ~{:.0} vectores  ·  residente ~{:.0}",
            50.0 / per,
            50.0 / per_mem
        );
        println!(
            "  la matriz entera ocupa        {:.2} MB",
            n as f64 * model.dim as f64 * 4.0 / 1_048_576.0
        );
    }

    // ── 2 · el rerank ──
    // No se mide `bge-reranker-v2-m3` porque no está descargado. Lo que sí se
    // puede medir sin bajar 2 GB es el **suelo**: 50 secuencias largas por un
    // modelo de 118 M de parámetros. Un cross-encoder de 568 M sobre el mismo
    // top-50 no puede ser más rápido que eso.
    let largo = "palabra ".repeat(400);
    let batch = embedder.batch_size().min(50);
    let lote: Vec<&str> = (0..batch).map(|_| largo.as_str()).collect();
    let t = std::time::Instant::now();
    let r = embedder.embed(&lote, obrain_embed::EmbedRole::Passage);
    let un_lote = t.elapsed().as_secs_f64();
    if r.is_ok() {
        let suelo = un_lote * (50.0 / batch as f64);
        println!("\n  50 secuencias largas por e5-small (118 M)  {suelo:.2} s  ← suelo MEDIDO");
        println!(
            "  bge-reranker-v2-m3 son 568 M               x4,8 ≈ {:.1} s  ← EXTRAPOLADO",
            suelo * 4.8
        );
        println!("  (la extrapolacion es por parametros; no es una medicion del reranker)");
    }
}

// ══════════════════════════════════════════════════════════════════ auditar

fn auditar(store: &Store, embedder: &dyn Embedder, tp: TimePoint) {
    let model = embedder.model();
    let vectors = match index::load_all(store.conn(), model) {
        Ok(v) => v,
        Err(e) => {
            println!("no se pudieron cargar los vectores: {e}");
            return;
        }
    };
    if vectors.is_empty() {
        println!("\n  (no hay vectores que auditar)");
        return;
    }
    let paths: HashMap<String, String> = vectors
        .iter()
        .map(|(_, e, _)| {
            let p = store
                .resolve(e, "path_observed_at", tp)
                .ok()
                .flatten()
                .and_then(|a| a.object.as_text().map(str::to_string))
                .unwrap_or_else(|| format!("<{e}>"));
            (e.to_string(), p)
        })
        .collect();
    let ruta = |e: &obrain_core::EntityId| -> String {
        corto(paths.get(e.as_str()).map(String::as_str).unwrap_or("?"))
    };

    // ── 1 · vecinos de documentos reales ──
    println!("\n══ VECINOS DE DOCUMENTOS REALES ═══════════");
    println!(
        "  El contador dice «{} vectores». Esto dice si significan algo.",
        vectors.len()
    );
    let interes = [
        "MatriculaJuanPabloGarcia",
        "PROJECT-MASTER",
        "docker-compose",
        "REVISION y PROPUESTA",
        "consultaExpte",
    ];
    for aguja in interes {
        let Some((chunk, entity, v)) = vectors
            .iter()
            .find(|(_, e, _)| paths.get(e.as_str()).is_some_and(|p| p.contains(aguja)))
        else {
            continue;
        };
        println!("\n  ▸ {}  (fragmento {chunk})", ruta(entity));
        let mut sims: Vec<(f32, &obrain_core::EntityId)> = vectors
            .iter()
            .filter(|(c, e, _)| c != chunk && e != entity)
            .map(|(_, e, w)| (cosine(v, w), e))
            .collect();
        sims.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut vistos: Vec<String> = Vec::new();
        for (s, e) in &sims {
            if vistos.contains(&e.to_string()) {
                continue;
            }
            vistos.push(e.to_string());
            println!("      {s:.3}  {}", ruta(e));
            if vistos.len() >= 4 {
                break;
            }
        }
    }

    // ── 2 · hubs ──
    // La patología que ningún contador delata: fragmentos que salen entre los
    // vecinos más cercanos de casi todo. En un índice con hubs la búsqueda
    // devuelve siempre lo mismo, y la cobertura sigue siendo del 100 %.
    println!("\n══ HUBS ═══════════════════════════════════");
    const TOP: usize = 10;
    let mut veces = vec![0usize; vectors.len()];
    let mut fila: Vec<(f32, usize)> = Vec::with_capacity(vectors.len());
    for (i, (_, ei, vi)) in vectors.iter().enumerate() {
        fila.clear();
        for (j, (_, ej, vj)) in vectors.iter().enumerate() {
            if i != j && ei != ej {
                fila.push((cosine(vi, vj), j));
            }
        }
        fila.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (_, j) in fila.iter().take(TOP) {
            veces[*j] += 1;
        }
    }
    let esperado = TOP as f64;
    let mut orden: Vec<usize> = (0..vectors.len()).collect();
    orden.sort_by_key(|i| std::cmp::Reverse(veces[*i]));
    println!("  Si el espacio fuera sano, cada fragmento saldria ~{esperado:.0} veces");
    println!("  entre los {TOP} vecinos mas cercanos de los demas.\n");
    println!("     veces  x esperado  fragmento");
    for i in orden.iter().take(10) {
        let (chunk, e, _) = &vectors[*i];
        println!(
            "     {:>5}  {:>10.0}  [{chunk}] {}",
            veces[*i],
            veces[*i] as f64 / esperado,
            ruta(e)
        );
    }
    let max = veces.iter().max().copied().unwrap_or(0);
    let nunca = veces.iter().filter(|v| **v == 0).count();
    println!(
        "\n  peor hub: {max} veces ({:.0}x lo esperado) · {nunca} fragmentos no son vecinos de nadie ({:.1} %)",
        max as f64 / esperado,
        100.0 * nunca as f64 / vectors.len() as f64
    );

    // ── 3 · el criterio de "hecho": BM25 frente a kNN ──
    println!("\n══ ENCONTRAR POR SIGNIFICADO, NO POR NOMBRE ═══");
    println!("  Consultas cuyas palabras NO estan en el documento objetivo.\n");
    for q in [
        "como demostre mis ingresos",
        "papeles de la universidad",
        "credenciales del servidor",
        "que decidimos sobre el almacen",
    ] {
        comparar(store, embedder, q, tp);
    }
}

/// Las tres listas para la misma consulta. Es la comparación que el informe
/// tiene que enseñar: decir que la fusión mejora sin enseñar lo que fusiona no
/// es una medición, es una nota de prensa.
fn comparar(store: &Store, embedder: &dyn Embedder, q: &str, tp: TimePoint) {
    let Ok((fused, lexical, vectorial)) = search::hybrid(store, embedder, q, 3, tp) else {
        return;
    };
    println!("  ─── «{q}»");
    print!("      BM25 ");
    if lexical.is_empty() {
        println!(" · NO ENCUENTRA NADA");
    } else {
        println!(" · {}", corto(&lexical[0].path));
    }
    print!("      kNN  ");
    if vectorial.is_empty() {
        println!(" · nada");
    } else {
        println!(
            " · {}  ({:.3})",
            corto(&vectorial[0].path),
            vectorial[0].score
        );
    }
    if let Some(f) = fused.first() {
        let proc: Vec<String> = f.ranks.iter().map(|(n, r)| format!("{n}#{r}")).collect();
        println!("      RRF   · {}  [{}]", corto(&f.path), proc.join(" "));
    }
    println!();
}
