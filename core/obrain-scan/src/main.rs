//! CLI de escaneo. Sirve para ejecutar M1 contra una máquina real y, sobre
//! todo, para **verificar** el criterio: cero hidratación de OneDrive.

use obrain_core::Store;
use obrain_scan::{ingest, now_ms, register_library, Availability, IngestReport, Scanner};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!(
            "obrain-scan — M1\n\n\
             USO: obrain-scan <ruta> [opciones]\n\n\
             OPCIONES:\n  \
               --db <archivo>   base de datos (por defecto: en memoria)\n  \
               --limit <n>      parar tras n archivos\n  \
               --dry-run        no escribir nada\n  \
               --no-hash        no leer contenido (solo metadatos)\n  \
               --verify         comprobar que ningun archivo de nube se hidrato\n"
        );
        return ExitCode::from(2);
    }

    // Absoluta siempre: la ruta de cada archivo **es** su carpeta para M3, y
    // `.\docs` y `C:\...\docs` serían dos árboles distintos para el mismo disco.
    let root = std::path::absolute(PathBuf::from(&args[0]))
        .unwrap_or_else(|_| PathBuf::from(&args[0]));
    let flag = |n: &str| args.iter().any(|a| a == n);
    let value = |n: &str| {
        args.iter()
            .position(|a| a == n)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };

    let dry_run = flag("--dry-run");
    let verify = flag("--verify");
    let no_hash = flag("--no-hash");

    if !root.exists() {
        eprintln!("no existe: {}", root.display());
        return ExitCode::FAILURE;
    }

    // ── 1 · descubrir + triar ──
    let mut scanner = Scanner::new();
    scanner.limit = value("--limit").and_then(|v| v.parse().ok());
    if no_hash {
        scanner.max_hash_bytes = 0;
    }

    println!("\n▸ escaneando {}", root.display());
    let (mut candidates, mut report) = scanner.scan(&root);

    // Instantánea de los archivos de nube ANTES de hashear.
    let cloud_before: Vec<PathBuf> = candidates
        .iter()
        .filter(|c| c.availability != Availability::Local)
        .map(|c| c.path.clone())
        .collect();

    // ── 2 · hashear (solo lo permitido por el triaje) ──
    scanner.hash_all(&mut candidates, &mut report);

    // ── 3 · ingestar ──
    let mut ing = IngestReport::default();
    if !dry_run {
        let db = value("--db");
        let mut store = match &db {
            Some(p) => Store::open(p),
            None => Store::open_in_memory(),
        }
        .expect("abrir store");

        let at = now_ms();
        let res = store.transaction(|s| {
            ingest(s, &candidates, at, &mut ing)?;
            // La raíz que se escanea es la que el usuario eligió: su biblioteca.
            register_library(s, &root, at).map(|_| ())
        });
        if let Err(e) = res {
            eprintln!("error al ingestar: {e}");
            return ExitCode::FAILURE;
        }
        if let Some(p) = &db {
            println!("  base de datos: {p}");
        }
    }

    // ── 4 · informe ──
    println!("\n── RECORRIDO ──────────────────────────────");
    println!("  directorios visitados   {}", report.dirs_visited);
    println!("  directorios PODADOS     {}", report.dirs_pruned);
    for (reason, n) in &report.prune_reasons {
        println!("      {reason:<28} {n}");
    }
    println!("  archivos vistos         {}", report.files_seen);
    println!("  archivos omitidos       {}", report.files_skipped);
    for (reason, n) in &report.skip_reasons {
        println!("      {reason:<28} {n}");
    }

    println!("\n── TRIAJE ─────────────────────────────────");
    for (k, n) in &report.by_availability {
        println!("  {k:<24} {n}");
    }

    println!("\n── ACCESO A DISCO ─────────────────────────");
    println!("  archivos abiertos       {}", report.files_opened);
    println!("  archivos leidos (hash)  {}", report.files_hashed);
    println!(
        "  bytes leidos            {:.1} MB",
        report.bytes_hashed as f64 / 1_048_576.0
    );
    println!("  archivos de nube NO tocados  {}", report.cloud_untouched);

    if !dry_run {
        println!("\n── LOG DE ASERCIONES ──────────────────────");
        println!("  entidades nuevas        {}", ing.entities_created);
        println!("  entidades reconocidas   {}", ing.entities_recognised);
        println!(
            "  entidades reconciliadas {}  ← guardado atomico: cambio el FileID",
            ing.entities_reconciled
        );
        println!("  aserciones escritas     {}", ing.assertions_written);
        println!("  aserciones sin cambio   {}", ing.assertions_unchanged);
        println!("  file_id retractados     {}", ing.assertions_retracted);
    }

    println!("\n  duracion  {:.2} s", report.elapsed.as_secs_f64());
    if !report.errors.is_empty() {
        println!("  errores   {} (primeros 3)", report.errors.len());
        for e in report.errors.iter().take(3) {
            println!("      {e}");
        }
    }

    // ── 5 · verificación del criterio de M1 ──
    if verify {
        println!("\n── VERIFICACION: ¿se hidrato algo? ────────");
        let mut hydrated = Vec::new();
        for p in &cloud_before {
            let attrs = match std::fs::symlink_metadata(p) {
                Ok(m) => attributes(&m),
                Err(_) => continue,
            };
            if obrain_scan::triage::classify(attrs) == Availability::Local {
                hydrated.push(p.clone());
            }
        }
        println!("  archivos de nube comprobados  {}", cloud_before.len());
        println!("  hidratados por el escaneo     {}", hydrated.len());

        if cloud_before.is_empty() {
            println!("\n  ⚠  INCONCLUSO — no habia archivos de nube en esta ruta.");
            println!("     Prueba con una carpeta de OneDrive con Archivos a Peticion.");
        } else if hydrated.is_empty() {
            println!("\n  ✓  PASA — ni un solo archivo de nube se descargo.");
        } else {
            println!("\n  ✗  FALLA — se hidrataron {} archivos:", hydrated.len());
            for p in hydrated.iter().take(5) {
                println!("      {}", p.display());
            }
            return ExitCode::FAILURE;
        }
    }

    println!();
    ExitCode::SUCCESS
}

#[cfg(windows)]
fn attributes(m: &std::fs::Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    m.file_attributes()
}

#[cfg(not(windows))]
fn attributes(_m: &std::fs::Metadata) -> u32 {
    0
}
