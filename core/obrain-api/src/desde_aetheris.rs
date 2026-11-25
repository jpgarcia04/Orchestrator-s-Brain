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

/// La ruta del documento `id` de la base `db`, si la puerta podría servirlo,
/// o el motivo fijo por el que no: `apagada`, `fuera_de_la_puerta`, los de
/// la apertura (`base_cerrada`, `ocupada`, `version_distinta`) o `fallo`.
pub fn ruta(db: &Path, id: &str) -> Result<String, &'static str> {
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
        assert_eq!(ruta(db, "nada"), Err("fuera_de_la_puerta"));
        assert!(ruta(db, ID).is_err());
    }
}
