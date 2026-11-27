//! **Abrir, revelar y mostrar a pedido de Aetheris** (ADR-0041).
//!
//! `obrain-app.exe --abrir <id>`, `--revelar <id>` y `--mostrar <id>` son
//! modos de arranque al lado de `--puerta`, y no herramientas de la puerta:
//! la puerta es de modelos, y un modelo no debe poder abrir un archivo. Los
//! lanza la ventana de Aetheris cuando JP pulsa un botón.
//!
//! Aquí está lo que no depende de Tauri: leer los argumentos y decidir, con
//! la apertura de sólo lectura de la puerta ([`crate::lectura`]), si un `id`
//! es algo que la puerta podría servir y cuál es su ruta
//! ([`crate::puerta::ruta_para_abrir`]). Abrir el archivo, revelarlo o llevar
//! la ventana a su ficha lo hace `ui/src-tauri`, que tiene con qué.
//!
//! Lo que nunca sale de aquí es la ruta: quien llama sólo sabe si salió bien
//! o el motivo fijo por el que no.

use crate::puerta::{self, NoSale};
use obrain_core::{Fallo, TimePoint};
use std::path::Path;

/// Qué pidió Aetheris.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accion {
    /// Con el programa que el sistema tenga asociado, como el botón «Abrir».
    Abrir,
    /// Su carpeta en el explorador, con el archivo elegido.
    Revelar,
    /// La ventana de Brain, en la ficha del documento.
    Mostrar,
}

impl Accion {
    /// La acción de un argumento de arranque, o `None` si no es ninguna.
    pub fn desde_argumento(arg: &str) -> Option<Self> {
        match arg {
            "--abrir" => Some(Accion::Abrir),
            "--revelar" => Some(Accion::Revelar),
            "--mostrar" => Some(Accion::Mostrar),
            _ => None,
        }
    }
}

/// Lo que un pedido puede contestar cuando no se abre nada. Fijo, como los
/// `no_disponible` de la puerta: no dice si el documento existe.
pub const ARGUMENTOS: &str = "argumentos";

/// Lee los argumentos de arranque (sin el programa). `None` si no son un
/// pedido de Aetheris; `Some(Err)` si lo son pero están mal formados.
pub fn pedido(args: &[String]) -> Option<Result<(Accion, String), &'static str>> {
    let accion = Accion::desde_argumento(args.first()?)?;
    Some(match args {
        [_, id] if es_id(id) => Ok((accion, id.clone())),
        _ => Err(ARGUMENTOS),
    })
}

/// ¿Tiene `id` la forma de un identificador de Brain? Un ULID: 26 caracteres
/// del alfabeto de Crockford, en mayúsculas. Lo que no la tiene ni siquiera
/// abre la base.
pub fn es_id(id: &str) -> bool {
    id.len() == 26
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b.is_ascii_uppercase() && !b"ILOU".contains(&b)))
}

/// El motivo fijo de un `--abrir` sobre algo que la puerta serviría pero que
/// no es un documento de [`DOCUMENTOS`] (ADR-0042). Sólo sale después de que
/// el `id` pasó la puerta, así que no dice nada que `--revelar` no diga ya.
pub const NO_ES_DOCUMENTO: &str = "no_es_documento";

/// **Lo único que `--abrir` abre** (ADR-0042): una lista permitida, nunca una
/// de prohibidas, en minúsculas. Abrir con el programa asociado ejecuta un
/// `.exe`, un `.lnk`, un `.bat` o un `.url`; también un `.html` o un `.svg`
/// llevan código al navegador. Lo que no está aquí se enseña con `--revelar`
/// o se abre desde la ventana de Brain.
pub const DOCUMENTOS: &[&str] = &[
    "pdf", "txt", "md", "csv", "doc", "docx", "odt", "rtf", "xls", "xlsx", "ods", "pptx", "odp",
    "jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "heic",
];

/// ¿Es `ruta` el nombre de un documento que `--abrir` puede abrir? Se juzga
/// por el texto del último componente, como lo juzga Windows al elegir el
/// programa. La extensión es la que va tras el último punto, sin distinguir
/// mayúsculas: `factura.PDF.exe` es un `.exe`. Un punto o un espacio final,
/// que Windows quita (`doc.pdf.`), dejan una extensión que no está en la
/// lista. Además se rechaza un `:`, que en NTFS nombra un flujo alternativo
/// (`programa.exe:x.pdf` es un flujo de un ejecutable), y cualquier carácter
/// de control.
pub fn es_documento(ruta: &str) -> bool {
    let nombre = ruta.rsplit(['/', '\\']).next().unwrap_or("");
    if nombre.contains(':') || nombre.chars().any(char::is_control) {
        return false;
    }
    match nombre.rsplit_once('.') {
        Some((base, ext)) if !base.is_empty() => {
            DOCUMENTOS.contains(&ext.to_ascii_lowercase().as_str())
        }
        _ => false,
    }
}

/// ¿Es lo que hay en `ruta` un archivo normal? Ni carpeta, ni enlace
/// simbólico (que abriría su destino, con otro nombre), ni nada que no esté.
/// Mira el disco con `symlink_metadata`, que no sigue el enlace.
fn es_archivo_normal(ruta: &str) -> bool {
    std::fs::symlink_metadata(ruta).is_ok_and(|m| m.file_type().is_file())
}

/// La ruta del documento `id` de la base `db` para `accion`, si la puerta
/// podría servirlo, o el motivo fijo por el que no: `apagada`,
/// `fuera_de_la_puerta`, los de la apertura (`base_cerrada`, `ocupada`,
/// `version_distinta`), `fallo` y, sólo para [`Accion::Abrir`],
/// [`NO_ES_DOCUMENTO`] (ADR-0042). Pide la acción para que nadie pueda pedir
/// la ruta de «revelar» y abrirla.
pub fn ruta(accion: Accion, db: &Path, id: &str) -> Result<String, &'static str> {
    let ruta = ruta_servible(db, id)?;
    if accion == Accion::Abrir && !(es_documento(&ruta) && es_archivo_normal(&ruta)) {
        return Err(NO_ES_DOCUMENTO);
    }
    Ok(ruta)
}

/// La ruta de lo que la puerta podría servir, sin mirar la acción.
fn ruta_servible(db: &Path, id: &str) -> Result<String, &'static str> {
    if !es_id(id) {
        return Err(NoSale::FueraDeLaPuerta.motivo());
    }
    let lectura = crate::lectura::abrir_para_leer(db).map_err(|no| no.motivo())?;
    let at = ahora();
    let mut fuera = None;
    let leido = lectura.leer(|s| {
        fuera = Some(puerta::ruta_para_abrir(s, id, at));
        Ok(())
    });
    drop(lectura);
    match (leido, fuera) {
        (Ok(()), Some(Ok(Ok(ruta)))) => Ok(ruta),
        (Ok(()), Some(Ok(Err(no)))) => Err(no.motivo()),
        (Err(Fallo::NoDisponible(no)), _) => Err(no.motivo()),
        _ => Err("fallo"),
    }
}

fn ahora() -> TimePoint {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    TimePoint::at(ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    const ID: &str = "01JD0000000000000000000000";

    #[test]
    fn solo_un_modo_y_un_identificador_son_un_pedido() {
        assert_eq!(pedido(&args(&[])), None);
        assert_eq!(pedido(&args(&["--puerta"])), None);
        assert_eq!(
            pedido(&args(&["--abrir", ID])),
            Some(Ok((Accion::Abrir, ID.into())))
        );
        assert_eq!(
            pedido(&args(&["--revelar", ID])),
            Some(Ok((Accion::Revelar, ID.into())))
        );
        assert_eq!(
            pedido(&args(&["--mostrar", ID])),
            Some(Ok((Accion::Mostrar, ID.into())))
        );
        assert_eq!(pedido(&args(&["--abrir"])), Some(Err(ARGUMENTOS)));
        assert_eq!(pedido(&args(&["--abrir", ID, "x"])), Some(Err(ARGUMENTOS)));
        assert_eq!(
            pedido(&args(&["--abrir", "C:\\Users\\x.txt"])),
            Some(Err(ARGUMENTOS))
        );
    }

    #[test]
    fn un_identificador_es_un_ulid_y_nada_mas() {
        assert!(es_id(ID));
        assert!(!es_id(""));
        assert!(!es_id("01jd0000000000000000000000"), "minúsculas");
        assert!(
            !es_id("01JD000000000000000000000I"),
            "una I no es Crockford"
        );
        assert!(!es_id("01JD00000000000000000000000"), "27 caracteres");
        assert!(!es_id("../../../../../../../../.."));
    }

    #[test]
    fn sin_base_no_hay_ruta_y_lo_mal_formado_ni_la_busca() {
        let db = Path::new("/no/existe/brain.obrain");
        for accion in [Accion::Abrir, Accion::Revelar, Accion::Mostrar] {
            assert_eq!(ruta(accion, db, "nada"), Err("fuera_de_la_puerta"));
            assert!(ruta(accion, db, ID).is_err());
        }
    }

    #[test]
    fn abrir_solo_abre_documentos_de_la_lista() {
        for si in [
            "factura.pdf",
            "FACTURA.PDF",
            "notas.md",
            "foto.JPG",
            "C:\\Users\\ana\\Documentos\\informe.Docx",
            "/home/ana/hoja.v2.xlsx",
        ] {
            assert!(es_documento(si), "{si}");
        }
        for no in [
            "factura.PDF.exe",
            "doc.pdf.lnk",
            "doc.pdf.",
            "doc.pdf ",
            "doc.pdf:flujo",
            "doc.pdf::$DATA",
            "C:\\docs\\doc.pdf:x",
            "sin_extension",
            ".pdf",
            "C:\\docs\\carpeta.pdf\\",
            "/home/ana/carpeta/",
            "",
            "acceso.url",
            "acceso.lnk",
            "programa.exe",
            "script.bat",
            "script.cmd",
            "script.ps1",
            "script.vbs",
            "script.js",
            "app.hta",
            "instalar.msi",
            "pantalla.scr",
            "registro.reg",
            "app.jar",
            "pagina.html",
            "pagina.htm",
            "dibujo.svg",
            "macro.docm",
            "doc.pdf\nexe",
            "doc\n.pdf",
            "programa.exe:x.pdf",
            "C:\\docs\\programa.exe:x.pdf",
        ] {
            assert!(!es_documento(no), "{no:?}");
        }
    }

    #[test]
    fn abrir_solo_abre_un_archivo_normal() {
        let dir = std::env::temp_dir().join(format!(
            "obrain-abrir-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(dir.join("carpeta.pdf")).unwrap();
        let archivo = dir.join("factura.pdf");
        std::fs::write(&archivo, b"%PDF-1.7 inventado").unwrap();
        let texto = |p: &Path| p.to_str().unwrap().to_string();
        assert!(es_archivo_normal(&texto(&archivo)));
        assert!(!es_archivo_normal(&texto(&dir.join("carpeta.pdf"))));
        assert!(!es_archivo_normal(&texto(&dir.join("no-esta.pdf"))));
        #[cfg(unix)]
        {
            let enlace = dir.join("enlace.pdf");
            std::os::unix::fs::symlink(&archivo, &enlace).unwrap();
            assert!(!es_archivo_normal(&texto(&enlace)));
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
