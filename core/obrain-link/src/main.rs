//! CLI de enlazado. Lee el log que dejó M1 y escribe las aristas deterministas.

use obrain_core::{Store, TimePoint};
use obrain_link::{link_all, load_files};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("obrain-link — M3\n\nUSO: obrain-link <base-de-datos>\n");
        return ExitCode::from(2);
    }

    let mut store = match Store::open(&args[0]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("no se pudo abrir {}: {e}", args[0]);
            return ExitCode::FAILURE;
        }
    };

    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let before = store.log_len().unwrap_or(0);

    let files = match load_files(&store, TimePoint::at(at)) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error al cargar: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!("\n▸ {} archivos en el log", files.len());

    let started = std::time::Instant::now();
    let report = match store.transaction(|s| link_all(s, &files, at)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error al enlazar: {e}");
            return ExitCode::FAILURE;
        }
    };
    let elapsed = started.elapsed();
    let after = store.log_len().unwrap_or(0);

    println!("\n── ARISTAS DETERMINISTAS ──────────────────");
    println!("  duplicate_of      {}", report.duplicate_of);
    println!("  contained_by      {}", report.contained_by);
    println!("  downloaded_from   {}", report.downloaded_from);
    println!("── HEURISTICAS (corregibles) ──────────────");
    println!("  occurred_during   {}", report.occurred_during);
    println!("  version_of        {}", report.version_of);
    println!("───────────────────────────────────────────");
    println!("  ARISTAS TOTALES   {}", report.edges_total());
    println!("  nodos concentradores creados  {}", report.hubs_created);
    println!("  aristas ya existentes         {}", report.already_present);

    println!("\n── LOG ────────────────────────────────────");
    println!("  aserciones antes  {before}");
    println!("  aserciones ahora  {after}");
    println!("  duracion          {:.2} s", elapsed.as_secs_f64());

    // La métrica de §0 del maestro: ¿cuánto del grafo lo autoró el sistema?
    if !files.is_empty() {
        println!(
            "\n  densidad: {:.2} aristas por archivo",
            report.edges_total() as f64 / files.len() as f64
        );
    }

    if !report.errors.is_empty() {
        println!("\n  errores: {}", report.errors.len());
    }

    // Auditoría: mirar aristas reales, no sólo contadores.
    if args.iter().any(|a| a == "--sample") {
        let tp = TimePoint::at(at);
        for p in [
            "duplicate_of",
            "downloaded_from",
            "version_of",
            "occurred_during",
        ] {
            println!("\n── muestra de {p} ──");
            match obrain_link::sample_edges(&store, p, 6, tp) {
                Ok(rows) if rows.is_empty() => println!("  (ninguna)"),
                Ok(rows) => rows.iter().for_each(|r| println!("  {r}")),
                Err(e) => println!("  error: {e}"),
            }
        }
    }

    println!();
    ExitCode::SUCCESS
}
