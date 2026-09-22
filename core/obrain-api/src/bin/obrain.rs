//! `obrain`: tu segundo cerebro desde la terminal.
//!
//! Lo que antes eran cinco programas en orden y una variable de entorno:
//!
//! ```text
//! obrain indexar "C:\Users\pacox\Segundo Cerebro"
//! ```
//!
//! y la aplicación lo ve al abrirse, porque las dos usan la misma base
//! ([`obrain_api::default_db_path`]). Las herramientas de cada etapa
//! (`obrain-scan`, `obrain-extract`…) siguen ahí para auditarlas.

use obrain_api::{default_db_path, indexar, load_model, Brain, Embedder, IndexReport};
use obrain_core::{Sensitivity, Store};
use std::path::PathBuf;
use std::process::ExitCode;

const AYUDA: &str = "\
obrain — tu segundo cerebro desde la terminal

USO:
  obrain indexar <carpeta> [<carpeta>...]   añade esas carpetas como bibliotecas y las indexa
  obrain indexar                             vuelve a indexar todas las bibliotecas
  obrain bibliotecas                         lista las bibliotecas registradas
  obrain sensible <carpeta>                  marca una carpeta como sensible: todo lo que
                                             tenga dentro lo hereda, también lo que llegue
  obrain sensible <carpeta> --boveda         lo mismo, pero como bóveda
  obrain sensible <carpeta> --quitar         le quita la marca
  obrain sensibles                           lista las carpetas marcadas

OPCIONES:
  --db <archivo>   usar otra base (por defecto, la misma que la aplicación)
  --sin-modelo     no vectorizar aunque el modelo esté descargado
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprint!("{AYUDA}");
        return ExitCode::from(2);
    }

    // Las opciones y sus valores fuera; lo que queda son la orden y sus carpetas.
    let mut db: Option<PathBuf> = None;
    let mut sin_modelo = false;
    let mut boveda = false;
    let mut quitar = false;
    let mut sueltos: Vec<&str> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--db" => db = it.next().map(PathBuf::from),
            "--sin-modelo" => sin_modelo = true,
            "--boveda" => boveda = true,
            "--quitar" => quitar = true,
            otro if otro.starts_with("--") => {
                eprintln!("opción desconocida: {otro}\n\n{AYUDA}");
                return ExitCode::from(2);
            }
            otro => sueltos.push(otro),
        }
    }
    let db = db.unwrap_or_else(default_db_path);
    let Some((orden, carpetas)) = sueltos.split_first() else {
        eprint!("{AYUDA}");
        return ExitCode::from(2);
    };

    if let Some(dir) = db.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut store = match Store::open(&db) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("no se pudo abrir la base {}: {e}", db.display());
            return ExitCode::FAILURE;
        }
    };

    match *orden {
        "bibliotecas" => match indexar::libraries(&store) {
            Ok(v) if v.is_empty() => {
                println!("todavía no hay bibliotecas. Añade una con:\n  obrain indexar <carpeta>");
                ExitCode::SUCCESS
            }
            Ok(v) => {
                for ruta in v {
                    println!("  {ruta}");
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        "sensible" => {
            let [carpeta] = carpetas else {
                eprintln!("uso: obrain sensible <carpeta> [--boveda | --quitar]");
                return ExitCode::from(2);
            };
            let nivel = match (quitar, boveda) {
                (true, _) => None,
                (false, true) => Some(Sensitivity::Vault),
                (false, false) => Some(Sensitivity::Sensitive),
            };
            marcar(store, carpeta, nivel)
        }
        "sensibles" => {
            let b = Brain::from_store(store);
            match b.marked_folders() {
                Ok(v) if v.is_empty() => {
                    println!("no has marcado ninguna carpeta. Márcala con:\n  obrain sensible <carpeta>");
                    ExitCode::SUCCESS
                }
                Ok(v) => {
                    for m in v {
                        let nivel = if m.level == "vault" { "bóveda" } else { "sensible" };
                        println!("  {nivel:<9} {}  ({} documentos)", m.path, m.documents);
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        "indexar" => {
            let modelo = if sin_modelo {
                None
            } else {
                match load_model(None) {
                    Ok(m) => {
                        if m.is_none() {
                            println!(
                                "· sin modelo local: la búsqueda por significado queda para después.\n  \
                                 Se descarga una vez con: obrain-embed \"{}\" --descargar-modelo",
                                db.display()
                            );
                        }
                        m
                    }
                    Err(e) => {
                        println!("· no se pudo cargar el modelo ({e}); sigo sin él");
                        None
                    }
                }
            };
            let modelo = modelo.as_deref().map(|m| m as &dyn Embedder);

            let res = if carpetas.is_empty() {
                indexar::reindex(&mut store, modelo)
            } else {
                let roots: Vec<PathBuf> = carpetas.iter().map(PathBuf::from).collect();
                indexar::index(&mut store, &roots, modelo)
            };
            match res {
                Ok(r) => {
                    informe(&r, &db);
                    if r.libraries.is_empty() && r.missing.is_empty() {
                        println!("\ntodavía no hay bibliotecas. Añade una con:\n  obrain indexar <carpeta>");
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error al indexar: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        otra => {
            eprintln!("orden desconocida: {otra}\n\n{AYUDA}");
            ExitCode::from(2)
        }
    }
}

/// `obrain sensible <carpeta>`: la misma operación que el botón de la cabecera.
fn marcar(store: Store, carpeta: &str, nivel: Option<Sensitivity>) -> ExitCode {
    let mut b = Brain::from_store(store);
    let ruta = std::path::absolute(carpeta)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| carpeta.to_string());
    let foco = match b.folder_at(&ruta) {
        Ok(Some(f)) => f,
        Ok(None) => {
            eprintln!(
                "no conozco la carpeta {ruta}.\n\
                 Tiene que estar dentro de una biblioteca ya indexada (obrain indexar)."
            );
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    match b.mark_folder(&foco, nivel) {
        Ok(r) => {
            match r.level {
                Some(n) => println!(
                    "▸ {ruta}\n  marcada como {}: {} dentro, {} han cambiado de nivel",
                    if n == "vault" { "bóveda" } else { "sensible" },
                    r.under,
                    r.changed
                ),
                None => println!(
                    "▸ {ruta}\n  sin marca: {} han vuelto a lo que diga su nombre o su texto",
                    r.changed
                ),
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn informe(r: &IndexReport, db: &std::path::Path) {
    for b in &r.libraries {
        println!("▸ {b}");
    }
    for b in &r.missing {
        println!("✗ {b} — no está en el disco; se deja como estaba");
    }
    println!();
    println!("  archivos vistos        {}", r.files_seen);
    println!("  nuevos                 {}", r.files_new);
    println!("  documentos leídos      {}", r.documents_read);
    println!("  relaciones nuevas      {}", r.edges_new);
    println!("  sensibles · bóveda     {} · {}", r.sensitive, r.vault);
    if r.inherited > 0 {
        println!("    por su carpeta       {}", r.inherited);
    }
    if r.semantic {
        println!("  vectores nuevos        {}", r.vectors_new);
    }
    println!("  en {:.2} s → {}", r.elapsed_ms as f64 / 1000.0, db.display());
}
