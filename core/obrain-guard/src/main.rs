//! CLI de clasificación de sensibilidad. Se ejecuta sobre una base ya
//! escaneada y extraída.
//!
//! `--sample` vuelve a ser parte del trabajo y no una herramienta de
//! depuración, por el mismo motivo que en M1, M2 y M3 — con un agravante: aquí
//! los dos errores posibles no son simétricos. Clasificar de más deja un
//! archivo fuera del índice vectorial, y se arregla con un clic. Clasificar de
//! menos manda un pasaporte a un modelo remoto. Un contador que diga «37
//! archivos en bóveda» no permite juzgar ni lo uno ni lo otro; la lista de los
//! 37, con la regla que disparó cada uno, sí.

use obrain_core::{Sensitivity, Store, TimePoint};
use obrain_extract::{FileRow, load_files};
use obrain_guard::{Capability, GuardReport, Runs, Verdict, classify_all, gate, mark_by_user};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!(
            "obrain-guard — sensibilidad y egreso (I11)\n\n\
             USO: obrain-guard <base-de-datos> [opciones]\n\n\
             OPCIONES:\n  \
               --sample              listar cada clasificacion, no solo contarlas\n  \
               --dry-run             clasificar sin escribir nada\n  \
               --marcar <txt> <niv>  marcar a mano los archivos cuya ruta contenga <txt>\n  \
                                     <niv> = normal | sensitive | vault\n"
        );
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
    let tp = TimePoint::at(at);

    let files = match load_files(&store, tp) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error al cargar: {e}");
            return ExitCode::FAILURE;
        }
    };

    // ── modo corrección: el usuario manda (I5) ──
    if let Some(i) = args.iter().position(|a| a == "--marcar") {
        let (Some(patron), Some(nivel)) = (
            args.get(i + 1),
            args.get(i + 2)
                .map(String::as_str)
                .and_then(Sensitivity::parse),
        ) else {
            eprintln!("uso: --marcar <texto-de-la-ruta> <normal|sensitive|vault>");
            return ExitCode::from(2);
        };
        return marcar(&store, &files, patron, nivel, at);
    }

    let dry_run = args.iter().any(|a| a == "--dry-run");
    println!("\n▸ {} archivos en el log", files.len());
    if dry_run {
        println!("  (dry-run: no se escribe nada)");
    }

    let before = store.log_len().unwrap_or(0);
    let started = std::time::Instant::now();
    let mut report = GuardReport::default();
    let res = store.transaction(|s| classify_all(s, &files, at, dry_run, &mut report));
    let elapsed = started.elapsed();

    let verdicts = match res {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error al clasificar: {e}");
            return ExitCode::FAILURE;
        }
    };
    let after = store.log_len().unwrap_or(0);

    // ── informe ──
    println!("\n── CLASIFICACION ──────────────────────────");
    println!("  archivos mirados        {}", report.considered);
    println!("  con texto que escanear  {}", report.with_text);
    println!("      normal              {}", report.normal);
    println!("      sensitive           {}", report.sensitive);
    println!("      vault               {}", report.vault);

    println!("\n── DE DONDE SALIO LA SEÑAL ────────────────");
    println!("  del nombre o la carpeta {}", report.from_name);
    println!("  del contenido           {}", report.from_content);
    if !report.by_rule.is_empty() {
        println!("\n  {:>6}  regla", "n");
        let mut reglas: Vec<_> = report.by_rule.iter().collect();
        reglas.sort_by_key(|(r, n)| (std::cmp::Reverse(**n), **r));
        for (regla, n) in reglas {
            println!("  {n:>6}  {regla}");
        }
    }

    println!("\n── LA PUERTA ──────────────────────────────");
    let vectorizables = gate::embeddable(store.conn()).map(|v| v.len()).unwrap_or(0);
    println!("  entidades vectorizables {vectorizables}");
    println!("  fragmentos retenidos    {}", report.withheld_chunks);
    println!(
        "  censo de entidades      normal {} · sensitive {} · vault {}",
        report.census[0], report.census[1], report.census[2]
    );

    println!("\n── LOG ────────────────────────────────────");
    println!("  aserciones antes        {before}");
    println!("  aserciones ahora        {after}");
    println!("  escritas                {}", report.assertions_written);
    println!("  retractadas             {}", report.assertions_retracted);
    println!("  sin cambio              {}", report.assertions_unchanged);
    println!("  ganadas por el usuario  {}", report.user_overrides);
    println!("  duracion                {:.2} s", elapsed.as_secs_f64());

    println!("\n── ARITMETICA ─────────────────────────────");
    println!(
        "  {} normal + {} sensitive + {} vault = {} mirados",
        report.normal, report.sensitive, report.vault, report.considered
    );
    println!(
        "  {} por nombre + {} por contenido = {} clasificados",
        report.from_name,
        report.from_content,
        report.sensitive + report.vault
    );
    println!(
        "  {}",
        if report.arithmetic_holds() {
            "✓  cierra"
        } else {
            "✗  NO CIERRA — el informe miente"
        }
    );

    if args.iter().any(|a| a == "--sample") {
        sample(&store, &verdicts);
    }

    println!();
    if report.arithmetic_holds() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Auditoría: mirar cada clasificación, no el total.
fn sample(store: &Store, verdicts: &[Verdict]) {
    // Carpeta y archivo, no sólo el archivo: la lección de M1 y M2 fue que la
    // contaminación se reconoce por **dónde** está algo, y hay cuatro
    // `.env.example` en el corpus que sólo se distinguen por su carpeta.
    let corto = |p: &str| {
        let mut partes = p.rsplit(['\\', '/']);
        match (partes.next(), partes.next()) {
            (Some(f), Some(d)) => format!("{d}\\{f}"),
            (Some(f), None) => f.to_string(),
            _ => p.to_string(),
        }
    };

    for (titulo, nivel) in [
        (
            "BOVEDA — fuera del indice vectorial y del egreso",
            Sensitivity::Vault,
        ),
        ("SENSIBLE — solo modelos locales", Sensitivity::Sensitive),
    ] {
        let del_nivel: Vec<&Verdict> = verdicts
            .iter()
            .filter(|v| v.signal.level == nivel)
            .collect();
        println!("\n══ {titulo} ({}) ═══", del_nivel.len());
        if del_nivel.is_empty() {
            println!("  (ninguno)");
            continue;
        }
        for v in del_nivel.iter().take(40) {
            println!(
                "  [{:.2}] {:<46}  {}",
                v.signal.confidence,
                truncar(&corto(&v.path), 46),
                v.signal.rule
            );
        }
        if del_nivel.len() > 40 {
            println!("  … y {} mas", del_nivel.len() - 40);
        }
    }

    // ── la puerta, ejercida sobre un archivo real ──
    println!("\n══ LA PUERTA SOBRE UN ARCHIVO REAL ════════");
    let ejemplo = verdicts
        .iter()
        .find(|v| v.signal.level == Sensitivity::Vault)
        .or_else(|| verdicts.first());
    match ejemplo {
        None => println!("  (no hay ningun archivo clasificado)"),
        Some(v) => {
            let nivel = store.sensitivity(&v.entity).unwrap_or(Sensitivity::Vault);
            println!("  {}  →  {}", corto(&v.path), nivel.as_str());
            for (cap, runs, etiqueta) in [
                (Capability::Embedder, Runs::Local, "vectorizar en local"),
                (Capability::Embedder, Runs::Remote, "vectorizar en remoto"),
                (Capability::Reranker, Runs::Local, "reordenar en local"),
                (Capability::Answerer, Runs::Remote, "responder en remoto"),
            ] {
                let d = gate::check(store, &v.entity, cap, runs)
                    .unwrap_or(gate::Decision::Deny("error al leer la sensibilidad"));
                let simbolo = match d {
                    gate::Decision::Allow => "PERMITIDO".to_string(),
                    gate::Decision::NeedsConsent => "REQUIERE CONSENTIMIENTO".to_string(),
                    gate::Decision::Deny(r) => format!("DENEGADO — {r}"),
                };
                println!("      {etiqueta:<22} {simbolo}");
            }
        }
    }

    // ── lo que nadie pudo mirar por dentro ──
    println!("\n══ CLASIFICADOS SOLO POR EL NOMBRE ════════");
    println!(
        "  {} de {} veredictos no pudieron apoyarse en el contenido.",
        verdicts.iter().filter(|v| v.chunk.is_none()).count(),
        verdicts.len()
    );
    println!("  Son fotos, binarios y archivos deshidratados de OneDrive:");
    println!("  justo los que peor se dejan clasificar y peor se dejan ignorar.");
}

fn truncar(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    s.chars().take(n - 1).collect::<String>() + "…"
}

/// `--marcar`: escribe aserciones de método `user`, que ninguna pasada
/// posterior del clasificador puede desplazar.
fn marcar(store: &Store, files: &[FileRow], patron: &str, nivel: Sensitivity, at: i64) -> ExitCode {
    let objetivo: Vec<&FileRow> = files
        .iter()
        .filter(|f| f.path.to_lowercase().contains(&patron.to_lowercase()))
        .collect();
    if objetivo.is_empty() {
        println!("\n  ningun archivo contiene {patron:?} en su ruta");
        return ExitCode::SUCCESS;
    }
    println!(
        "\n▸ marcando {} archivos como {}",
        objetivo.len(),
        nivel.as_str()
    );
    for f in &objetivo {
        if let Err(e) = mark_by_user(store, &f.entity, nivel, at) {
            eprintln!("  error en {}: {e}", f.file_name());
            return ExitCode::FAILURE;
        }
        println!("  {}", f.file_name());
    }
    println!("\n  queda en el log como asercion de usuario: permanente (I5).");
    ExitCode::SUCCESS
}
