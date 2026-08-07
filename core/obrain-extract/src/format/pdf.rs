//! PDF. El formato que más importa y el que peor se porta.
//!
//! Dos cosas que hay que decir sin adornos:
//!
//! - **Un PDF escaneado no tiene texto.** Es una imagen dentro de un
//!   contenedor. Aquí sale vacío, y sale vacío *correctamente*: inventarse
//!   OCR sería otra cosa distinta y con otro coste. El informe lo cuenta.
//! - **La librería puede entrar en pánico** con PDF malformados. Un archivo
//!   raro no puede tumbar el indexado de los otros mil, así que la llamada va
//!   envuelta en `catch_unwind`. No es elegante; es lo correcto cuando el
//!   fallo viene de datos que no controlamos.

use std::panic::{catch_unwind, AssertUnwindSafe};

pub fn extract(bytes: &[u8]) -> Result<String, String> {
    let owned = bytes.to_vec();
    match catch_unwind(AssertUnwindSafe(move || {
        pdf_extract::extract_text_from_mem(&owned)
    })) {
        Ok(Ok(text)) => Ok(text),
        Ok(Err(e)) => Err(format!("pdf: {e}")),
        Err(_) => Err("pdf: panico en el parser".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lo que de verdad hay que garantizar: un PDF corrupto no tumba el proceso.
    #[test]
    fn un_pdf_roto_devuelve_error_no_tumba_el_indexado() {
        assert!(extract(b"%PDF-1.4\nesto no es un pdf").is_err());
        assert!(extract(b"").is_err());
    }
}
