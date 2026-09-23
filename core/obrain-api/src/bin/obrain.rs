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

use obrain_api::{Brain, Embedder, IndexReport, default_db_path, indexar, load_model, seguridad};
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
  obrain vencimientos                        lo que caduca en 90 días o ya caducó
  obrain papelera                            lo que ya no está en tus carpetas (30 días)
  obrain papelera --vaciar                   olvida ya lo que obrain leyó de ello (tus
                                             archivos no se tocan)
  obrain clave                               enseña la clave de recuperación de la base
  obrain clave --guardada                    dice que ya la guardaste (la app deja de pedirlo)
  obrain recuperar <clave>                   abre la base en un equipo que no tiene su llave

La base está siempre cifrada (SQLCipher). La primera vez que se abre —o si
venía de una versión en claro— se cifra y se enseña su clave de recuperación.

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
    let mut guardada = false;
    let mut vaciar = false;
    let mut sueltos: Vec<&str> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--db" => db = it.next().map(PathBuf::from),
            "--sin-modelo" => sin_modelo = true,
            "--boveda" => boveda = true,
            "--quitar" => quitar = true,
            "--guardada" => guardada = true,
            "--vaciar" => vaciar = true,
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
    // Recuperar va antes de abrir: es justo lo que hace falta cuando no se puede.
    if *orden == "recuperar" {
        let [codigo] = carpetas else {
            eprintln!("uso: obrain recuperar <clave-de-recuperación>");
            return ExitCode::from(2);
        };
        return match seguridad::recuperar(&db, codigo) {
            Ok(()) => {
                println!(
                    "▸ la base vuelve a abrirse en este equipo: {}",
                    db.display()
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }

    let mut store = match seguridad::abrir(&db) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("no se pudo abrir la base {}: {e}", db.display());
            if obrain_api::Error::from(e).is_key_unavailable() {
                eprintln!(
                    "\nSi tienes la clave de recuperación:\n  obrain recuperar XXXXX-XXXXX-XXXXX-XXXXX-XXXXX"
                );
            }
            return ExitCode::FAILURE;
        }
    };
    avisar_recuperacion(&store, *orden == "clave");

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
        "clave" => {
            let b = Brain::from_store(store);
            if guardada {
                return match b.confirm_recovery_saved() {
                    Ok(()) => {
                        println!("▸ anotado: la app ya no te la volverá a pedir");
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("{e}");
                        ExitCode::FAILURE
                    }
                };
            }
            match b.recovery_code() {
                Ok(Some(c)) => {
                    println!(
                        "  clave de recuperación de {}:\n\n      {c}\n",
                        db.display()
                    );
                    ExitCode::SUCCESS
                }
                Ok(None) => {
                    println!("esta base no tiene clave de recuperación guardada");
                    ExitCode::SUCCESS
                }
                Err(obrain_api::Error::Bloqueado) => {
                    eprintln!("tienes PIN: mira la clave desde la app, en «Seguridad»");
                    ExitCode::FAILURE
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
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
                    println!(
                        "no has marcado ninguna carpeta. Márcala con:\n  obrain sensible <carpeta>"
                    );
                    ExitCode::SUCCESS
                }
                Ok(v) => {
                    for m in v {
                        let nivel = if m.level == "vault" {
                            "bóveda"
                        } else {
                            "sensible"
                        };
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
        "vencimientos" => {
            let b = Brain::from_store(store);
            match b.expiring() {
                Ok(v) if v.is_empty() => {
                    println!("nada caduca en los próximos 90 días.");
                    ExitCode::SUCCESS
                }
                Ok(v) => {
                    for e in v {
                        let cuando = match e.days {
                            Some(d) if d < 0 => format!("caducó hace {}", dias(-d)),
                            Some(0) => "caduca hoy".to_string(),
                            Some(d) => format!("caduca en {}", dias(d)),
                            None => "fecha protegida con tu PIN".to_string(),
                        };
                        let fecha = e.date.as_deref().unwrap_or("··········");
                        let tuya = if e.by_user { "  (lo dijiste tú)" } else { "" };
                        println!("  {fecha}  {cuando:<22} {}{tuya}", e.row.path);
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        "papelera" => {
            let mut b = Brain::from_store(store);
            if vaciar {
                return match b.forget(None) {
                    Ok(0) => {
                        println!("la papelera ya estaba vacía.");
                        ExitCode::SUCCESS
                    }
                    Ok(n) => {
                        println!(
                            "olvidado lo que se leyó de {n} documento{}. Tus archivos no se han tocado.",
                            if n == 1 { "" } else { "s" }
                        );
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("{e}");
                        ExitCode::FAILURE
                    }
                };
            }
            match b.trash() {
                Ok(v) if v.is_empty() => {
                    println!("la papelera está vacía.");
                    ExitCode::SUCCESS
                }
                Ok(v) => {
                    for p in v {
                        let cuando = match p.days_left {
                            0 => "se olvida hoy".to_string(),
                            d => format!("se olvida en {}", dias(d)),
                        };
                        println!("  {cuando:<22} {}", p.path);
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
                        println!(
                            "\ntodavía no hay bibliotecas. Añade una con:\n  obrain indexar <carpeta>"
                        );
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

/// Si la base acaba de cifrarse (o nadie ha confirmado todavía que guardó la
/// clave), se enseña. Con PIN no: la clave se mira en la app, desbloqueada.
fn avisar_recuperacion(store: &Store, ya_la_pide: bool) {
    if ya_la_pide {
        return;
    }
    let Ok(estado) = seguridad::estado(store.conn(), 0) else {
        return;
    };
    if !estado.recuperacion_pendiente || estado.pin {
        return;
    }
    if let Ok(Some(c)) = seguridad::clave_de_recuperacion(store.conn()) {
        println!(
            "🔐 Tu base está cifrada. Apunta su clave de recuperación y guárdala en un sitio seguro:\n\n      \
             {c}\n\n   Sin ella, si Windows pierde la llave (otro equipo, Windows reinstalado), la base no se\n   \
             abre. Cuando la tengas guardada: obrain clave --guardada\n"
        );
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
    if r.ocr > 0 {
        println!("    con OCR              {}", r.ocr);
    }
    println!("  relaciones nuevas      {}", r.edges_new);
    println!("  sensibles · bóveda     {} · {}", r.sensitive, r.vault);
    if r.inherited > 0 {
        println!("    por su carpeta       {}", r.inherited);
    }
    if r.expiring > 0 {
        println!("  con fecha de caducidad {}", r.expiring);
    }
    if r.trashed > 0 {
        println!("  a la papelera          {}", r.trashed);
    }
    if r.returned > 0 {
        println!("  de vuelta              {}", r.returned);
    }
    if r.forgotten > 0 {
        println!("  olvidados (30 días)    {}", r.forgotten);
    }
    if r.semantic {
        println!("  vectores nuevos        {}", r.vectors_new);
    }
    println!(
        "  en {:.2} s → {}",
        r.elapsed_ms as f64 / 1000.0,
        db.display()
    );
}

/// `1 día`, `53 días`.
fn dias(n: i64) -> String {
    if n == 1 {
        "1 día".to_string()
    } else {
        format!("{n} días")
    }
}
