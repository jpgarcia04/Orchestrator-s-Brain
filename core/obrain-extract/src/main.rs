//! CLI de extracción. Se ejecuta sobre una base ya escaneada por `obrain-scan`.
//!
//! Como en M3, `--sample` no es una herramienta de depuración: es parte del
//! hito. Un extractor cuyos resultados no se pueden mirar es un extractor en el
//! que no se puede confiar, y los contadores no delatan que un PDF salió como
//! sopa de ligaduras ni que una página web indexó su aviso de cookies.

use obrain_core::{Store, TimePoint};
use obrain_extract::{extract_all_with, index, load_files, search, ExtractReport};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!(
            "obrain-extract — M2\n\n\
             USO: obrain-extract <base-de-datos> [opciones]\n\n\
             OPCIONES:\n  \
               --sample          mostrar extracciones y busquedas reales\n  \
               --buscar <texto>  buscar en el indice lexico y salir\n  \
               --pruebas         auditar que fragmentos tienen prueba que enseñar\n  \
               --limite <n>      resultados de busqueda (por defecto 10)\n"
        );
        return ExitCode::from(2);
    }

    let value = |n: &str| {
        args.iter()
            .position(|a| a == n)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };

    let mut store = match Store::open(&args[0]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("no se pudo abrir {}: {e}", args[0]);
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = index::migrate(store.conn()) {
        eprintln!("no se pudo crear el indice: {e}");
        return ExitCode::FAILURE;
    }

    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let tp = TimePoint::at(at);
    let limit = value("--limite").and_then(|v| v.parse().ok()).unwrap_or(10);

    // ── auditar la prueba: no extrae nada, sólo mide lo que hay ──
    //
    // Con salida temprana a propósito: medir si el índice puede explicarse no
    // debería obligar a reescribirlo. Es la misma forma de `--pesos` en M5a.
    if args.iter().any(|a| a == "--pruebas") {
        evidencia(&store, tp);
        println!();
        return ExitCode::SUCCESS;
    }

    // ── modo consulta: no extrae nada, sólo busca ──
    if let Some(q) = value("--buscar") {
        let t0 = std::time::Instant::now();
        let res = search(&store, &q, limit, tp);
        let elapsed = t0.elapsed();
        return match res {
            Ok(hits) => {
                print_hits(&q, &hits);
                // "Instantánea" es el criterio de M2. Un número, no un adjetivo.
                println!("\n  {} resultados en {:.1} ms", hits.len(), elapsed.as_secs_f64() * 1000.0);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error en la busqueda: {e}");
                ExitCode::FAILURE
            }
        };
    }

    let files = match load_files(&store, tp) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error al cargar: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!("\n▸ {} archivos en el log", files.len());

    // `pdf-extract` entra en pánico con algunos PDF malformados; lo capturamos
    // y lo contamos como fallo del archivo. Silenciamos el volcado para que un
    // corpus con diez PDF rotos no escupa diez trazas por consola — el informe
    // final los cuenta uno a uno.
    std::panic::set_hook(Box::new(|_| {}));

    let before = store.log_len().unwrap_or(0);
    let started = std::time::Instant::now();
    let mut report = ExtractReport::default();
    // El OCR del sistema, si lo hay (Windows): fotos y PDF escaneados.
    let ocr = obrain_ocr::platform();
    match &ocr {
        Some(o) => println!("  OCR: {}", o.engine()),
        None => println!("  OCR: no hay reconocedor en este sistema; las fotos se saltan"),
    }
    let res = store.transaction(|s| extract_all_with(s, &files, at, &mut report, ocr.as_deref()));
    let elapsed = started.elapsed();
    let _ = std::panic::take_hook();

    if let Err(e) = res {
        eprintln!("error al extraer: {e}");
        return ExitCode::FAILURE;
    }
    let after = store.log_len().unwrap_or(0);

    // ── informe ──
    println!("\n── DECIDIDO SIN ABRIR NADA ────────────────");
    println!("  formato no soportado    {}", report.skipped_unsupported);
    println!("  NO LOCAL (no se toco)   {}", report.skipped_not_local);
    println!("  demasiado grande        {}", report.skipped_too_large);
    println!("  ya extraido (sin cambio){:>4}", report.unchanged);

    println!("\n── ABIERTOS ───────────────────────────────");
    println!("  archivos abiertos       {}", report.opened);
    println!("      texto extraido      {}", report.extracted);
    println!("        con OCR           {}", report.ocr);
    println!("      sin texto (vacio)   {}", report.empty);
    println!("      binario disfrazado  {}", report.binary);
    println!("      ruido descartado    {}", report.noise);
    println!("      fallo el extractor  {}", report.failed);
    println!(
        "  bytes leidos            {:.1} MB",
        report.bytes_read as f64 / 1_048_576.0
    );

    println!("\n── INDICE LEXICO ──────────────────────────");
    println!("  fragmentos escritos     {}", report.chunks);
    println!("  caracteres indexados    {}", report.chars);
    println!(
        "  fragmentos en el indice {}",
        index::chunk_count(store.conn()).unwrap_or(-1)
    );
    if report.extracted > 0 {
        println!(
            "  media                   {:.1} fragmentos/archivo",
            report.chunks as f64 / report.extracted as f64
        );
    }
    // Coste real del solape: cuánto texto se guarda de más para que una frase
    // partida por un límite siga siendo encontrable entera.
    if let (Ok(guardados), true) = (index::stored_chars(store.conn()), report.chars > 0) {
        println!(
            "  solape                  {:.1}% ({guardados} guardados / {} de texto)",
            (guardados as f64 / report.chars as f64 - 1.0) * 100.0,
            report.chars
        );
    }

    println!("\n── POR FORMATO ────────────────────────────");
    println!(
        "  {:<12}{:>9}{:>11}{:>12}{:>8}{:>7}",
        "formato", "archivos", "fragmentos", "caracteres", "vacios", "ruido"
    );
    for (fmt, s) in &report.by_format {
        println!(
            "  {fmt:<12}{:>9}{:>11}{:>12}{:>8}{:>7}",
            s.files, s.chunks, s.chars, s.empty, s.noise
        );
    }

    println!("\n── LOG ────────────────────────────────────");
    println!("  aserciones antes        {before}");
    println!("  aserciones ahora        {after}");
    println!("  escritas                {}", report.assertions_written);
    println!("  sin cambio              {}", report.assertions_unchanged);
    println!("  duracion                {:.2} s", elapsed.as_secs_f64());

    // ── la aritmética, comprobada y no prometida ──
    let sin_abrir = report.skipped_unsupported
        + report.skipped_not_local
        + report.skipped_too_large
        + report.unchanged;
    println!("\n── ARITMETICA ─────────────────────────────");
    println!(
        "  {sin_abrir} sin abrir + {} abiertos = {} considerados",
        report.opened, report.considered
    );
    println!(
        "  {} ok + {} vacios + {} binarios + {} ruido + {} fallos = {} abiertos",
        report.extracted, report.empty, report.binary, report.noise, report.failed, report.opened
    );
    println!(
        "  {}",
        if report.arithmetic_holds() {
            "✓  cierra"
        } else {
            "✗  NO CIERRA — el informe miente"
        }
    );

    if !report.errors.is_empty() {
        println!("\n  errores {} (primeros 5)", report.errors.len());
        for e in report.errors.iter().take(5) {
            println!("      {e}");
        }
    }

    if args.iter().any(|a| a == "--sample") {
        sample(&store, &files, tp);
    }

    println!();
    if report.arithmetic_holds() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn print_hits(query: &str, hits: &[obrain_extract::Hit]) {
    println!("\n── BUSQUEDA: {query:?} ──");
    if hits.is_empty() {
        println!("  (sin resultados)");
        return;
    }
    for (i, h) in hits.iter().enumerate() {
        let name = h.path.rsplit(['\\', '/']).next().unwrap_or(&h.path);
        println!("\n  {}. [{:>7.2}] {name}", i + 1, h.score);
        println!("      {}", h.path);
        println!("      …{}…", h.snippet.replace('\n', " "));
    }
}

/// ¿Hay algo que enseñar como prueba en cada fragmento?
///
/// I9 dice que ninguna respuesta va sin procedencia, y esa procedencia es un
/// tramo de texto que se le enseña a una persona. Los contadores de arriba
/// dicen cuántos fragmentos hay; éste dice **cuántos podrían explicarse**, que
/// es lo único que se ve en pantalla. Auditar esto es lo que destapó, el
/// 2026-08-12, que la prueba de la pregunta de la visa era un sello del SAT en
/// base64 — con todos los contadores en verde.
fn evidencia(store: &Store, tp: TimePoint) {
    use obrain_extract::evidence;

    println!("\n══ PRUEBA QUE SE PUEDE ENSEÑAR ════════════");
    let mut stmt = match store
        .conn()
        .prepare("SELECT entity, ordinal, text FROM chunks ORDER BY id")
    {
        Ok(s) => s,
        Err(e) => return println!("  error: {e}"),
    };
    let filas = stmt.query_map([], |r| {
        Ok((
            obrain_core::EntityId(r.get::<_, String>(0)?),
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
        ))
    });
    let Ok(filas) = filas else {
        return println!("  error al leer los fragmentos");
    };

    let (mut total, mut con_sello, mut sin_prueba) = (0usize, 0usize, 0usize);
    let mut mudos: Vec<(obrain_core::EntityId, i64)> = Vec::new();
    for fila in filas.flatten() {
        let (entity, ordinal, text) = fila;
        total += 1;
        if !evidence::machine_spans(&text).is_empty() {
            con_sello += 1;
        }
        // Sin consulta: el peor caso, el que se enseña cuando no hay término
        // que resaltar. Si aquí no sale nada, no hay nada que enseñar nunca.
        if evidence::pick(&text, "", evidence::BUDGET).is_none() {
            sin_prueba += 1;
            if mudos.len() < 10 {
                mudos.push((entity, ordinal));
            }
        }
    }

    let pct = |n: usize| n as f64 * 100.0 / total.max(1) as f64;
    println!("  fragmentos                {total}");
    println!("  con texto de maquina      {con_sello}  ({:.1}%)", pct(con_sello));
    println!("  SIN nada que enseñar      {sin_prueba}  ({:.1}%)", pct(sin_prueba));
    if sin_prueba > 0 {
        println!("\n  los que no tienen prueba (se enseñan vacios, nunca el sello):");
        for (e, ordinal) in &mudos {
            let p = store
                .resolve(e, "path_observed_at", tp)
                .ok()
                .flatten()
                .and_then(|a| a.object.as_text().map(str::to_string))
                .unwrap_or_else(|| e.to_string());
            println!("    #{ordinal:<4} {}", p.rsplit(['\\', '/']).next().unwrap_or(&p));
        }
    }
}

/// Auditoría: mirar texto real, no contadores.
fn sample(store: &Store, files: &[obrain_extract::FileRow], tp: TimePoint) {
    println!("\n══ MUESTRA DE EXTRACCIONES ════════════════");

    // Un archivo por formato, el más gordo de cada uno: es donde se ve si la
    // extracción produjo prosa o basura.
    let mut per_format: std::collections::BTreeMap<&str, Vec<&obrain_extract::FileRow>> =
        Default::default();
    for f in files {
        if let Some(fmt) = obrain_extract::format::detect(&f.path) {
            per_format.entry(fmt.as_str()).or_default().push(f);
        }
    }

    for (fmt, mut group) in per_format {
        group.sort_by_key(|f| std::cmp::Reverse(f.size));
        println!("\n── {fmt} ──");
        let mut shown = 0;
        for f in group {
            if shown >= 3 {
                break;
            }
            let chunks = index::chunks_of(store.conn(), &f.entity).unwrap_or_default();
            if chunks.is_empty() {
                continue;
            }
            shown += 1;
            println!("\n  {}  ({} fragmentos)", f.file_name(), chunks.len());
            let text = &chunks[0].1;
            let head: String = text.chars().take(240).collect();
            println!("  ┌─────────────────────────────────────────");
            for line in head.lines().take(6) {
                println!("  │ {line}");
            }
            println!("  └─────────────────────────────────────────");
        }
        if shown == 0 {
            println!("  (ningun archivo de este formato produjo texto)");
        }
    }

    // ── quién ocupa el índice de verdad ──
    let nombre = |e: &obrain_core::EntityId| {
        store
            .resolve(e, "path_observed_at", tp)
            .ok()
            .flatten()
            .and_then(|a| a.object.as_text().map(str::to_string))
            .unwrap_or_else(|| e.to_string())
    };

    println!("\n══ QUIEN OCUPA EL INDICE ══════════════════");
    let total = index::chunk_count(store.conn()).unwrap_or(1).max(1);
    match index::heaviest(store.conn(), 12) {
        Ok(rows) => {
            let acumulado: i64 = rows.iter().map(|(_, n, _)| n).sum();
            println!("  {:>7} {:>7}  archivo", "frag.", "% ind.");
            for (e, n, _) in &rows {
                let p = nombre(e);
                let corto = p.rsplit(['\\', '/']).next().unwrap_or(&p);
                println!("  {n:>7} {:>6.1}%  {corto}", *n as f64 * 100.0 / total as f64);
            }
            println!(
                "\n  estos 12 archivos son el {:.1}% del indice ({acumulado} de {total} fragmentos)",
                acumulado as f64 * 100.0 / total as f64
            );
        }
        Err(e) => println!("  error: {e}"),
    }

    for (titulo, estado) in [
        ("ABIERTOS Y SIN TEXTO", "empty"),
        ("DESCARTADO POR MINIFICADO", "minified"),
        ("DESCARTADO POR SER UN DICCIONARIO", "wordlist"),
        ("BINARIO DISFRAZADO DE TEXTO", "binary"),
    ] {
        println!("\n══ {titulo} ═══");
        match index::with_status(store.conn(), estado, 10) {
            Ok(rows) if rows.is_empty() => println!("  (ninguno)"),
            Ok(rows) => {
                for e in &rows {
                    let p = nombre(e);
                    println!("  {}", p.rsplit(['\\', '/']).next().unwrap_or(&p));
                }
            }
            Err(e) => println!("  error: {e}"),
        }
    }

    evidencia(store, tp);

    println!("\n══ BUSQUEDAS DE PRUEBA ════════════════════");
    for q in [
        // el nombre del archivo no dice "matrícula"… su contenido sí
        "matricula",
        "aceite de semilla",
        "asercion bitemporal",
        // el archivo se llama `consultaExpte_…`: "expediente" sólo está dentro
        "expediente",
        // una palabra corriente: sirve para ver si el ruido tapa lo bueno
        "trabajo",
    ] {
        match search(store, q, 3, tp) {
            Ok(hits) => print_hits(q, &hits),
            Err(e) => println!("\n  {q:?}: error {e}"),
        }
    }
}
