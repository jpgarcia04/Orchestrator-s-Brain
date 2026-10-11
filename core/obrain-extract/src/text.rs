//! De bytes a texto normalizado.
//!
//! Dos responsabilidades, y las dos son de las que arruinan un índice si se
//! hacen mal:
//!
//! 1. **Decidir si esto es texto siquiera.** Un `.log` de LevelDB y un `.txt`
//!    de notas tienen la misma extensión de cara al sistema operativo. La
//!    diferencia está en los bytes.
//! 2. **Normalizar antes de medir nada.** Todos los offsets, el hash del texto
//!    y los fragmentos se refieren al texto *ya normalizado*. Si se normaliza
//!    después, las evidencias apuntan a posiciones que no existen.

/// Cuántos bytes se miran para decidir si un archivo es binario.
const SNIFF: usize = 8192;

/// Resultado de interpretar unos bytes como texto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decoded {
    Text(String),
    /// Hay bytes que ningún documento de texto tendría. No se indexa.
    Binary,
}

/// Interpreta unos bytes como texto, o los declara binarios.
///
/// El orden de las comprobaciones es el que importa:
/// BOM → **olfateo de binario** → UTF-8 válido → Windows-1252.
///
/// El olfateo va **antes** que la validación de UTF-8, y eso no es un detalle
/// de estilo: `\0` es un carácter UTF-8 perfectamente válido. Con el orden
/// intuitivo —validar primero, olfatear si falla— un `.log` de LevelDB, que es
/// ASCII salpicado de nulos, pasa la validación y entra entero en el índice.
/// Costó tres tests en rojo descubrirlo.
///
/// Windows-1252 como último recurso y no ISO-8859-1 porque este corpus es
/// español y de Windows: las comillas tipográficas y el `€` de un `.txt`
/// guardado por el Bloc de notas viven en 0x80–0x9F, que en ISO-8859-1 son
/// caracteres de control.
pub fn decode(bytes: &[u8]) -> Decoded {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return Decoded::Text(String::from_utf8_lossy(rest).into_owned());
    }
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        let big = bytes[0] == 0xFE;
        return Decoded::Text(decode_utf16(&bytes[2..], big));
    }
    if looks_binary(bytes) {
        return Decoded::Binary;
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Decoded::Text(s.to_string());
    }
    Decoded::Text(encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned())
}

/// Un byte nulo, o demasiados caracteres de control, en la cabecera.
///
/// El nulo es la señal clásica y casi no tiene falsos positivos en texto
/// real — salvo UTF-16 sin BOM, que aquí se declara binario a conciencia:
/// preferimos no indexar un archivo a llenar el índice de basura.
pub fn looks_binary(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(SNIFF)];
    if head.is_empty() {
        return false;
    }
    if head.contains(&0) {
        return true;
    }
    let control = head
        .iter()
        .filter(|&&b| b < 0x20 && b != b'\t' && b != b'\n' && b != b'\r')
        .count();
    control * 100 > head.len() * 5
}

fn decode_utf16(bytes: &[u8], big_endian: bool) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| {
            if big_endian {
                u16::from_be_bytes([c[0], c[1]])
            } else {
                u16::from_le_bytes([c[0], c[1]])
            }
        })
        .collect();
    String::from_utf16_lossy(&units)
}

/// Texto canónico: es lo que se hashea, se fragmenta y a lo que apuntan todos
/// los offsets.
///
/// - saltos de línea a `\n` (un `.txt` de Windows si no, duplica cada offset)
/// - controles invisibles fuera (los PDF sueltan `\x0c` entre páginas)
/// - como mucho una línea en blanco seguida: el separador de párrafo es
///   información, tres líneas en blanco no lo son
/// - sin espacios al final de línea ni al principio o final del texto
pub fn normalize(raw: &str) -> String {
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    // CRLF primero y entero: tratar el `\r` suelto produciría dos saltos.
    let unified = raw.replace("\r\n", "\n");
    let mut out = String::with_capacity(unified.len());

    for ch in unified.chars() {
        match ch {
            '\r' => out.push('\n'),
            '\n' | '\t' => out.push(ch),
            // separadores Unicode de línea y de párrafo
            '\u{2028}' | '\u{2029}' => out.push('\n'),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }

    let mut result = String::with_capacity(out.len());
    let mut blank_run = 0usize;
    for line in out.split('\n') {
        let line = line.trim_end();
        if line.is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        result.push_str(line);
        result.push('\n');
    }
    result.trim().to_string()
}

/// Hash del texto normalizado. Permite saltarse la fragmentación cuando el
/// archivo cambió pero su texto no — reordenar un PDF, reguardar un `.docx`.
pub fn text_hash(text: &str) -> String {
    blake3::hash(text.as_bytes()).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_con_bom_pierde_el_bom() {
        let mut b = vec![0xEF, 0xBB, 0xBF];
        b.extend_from_slice("hola".as_bytes());
        assert_eq!(decode(&b), Decoded::Text("hola".into()));
    }

    #[test]
    fn utf16_con_bom_se_decodifica() {
        let mut b = vec![0xFF, 0xFE];
        for u in "añ".encode_utf16() {
            b.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(decode(&b), Decoded::Text("añ".into()));
    }

    #[test]
    fn latin1_de_windows_no_es_binario() {
        // "sesión" en Windows-1252: la ó es 0xF3, que no es UTF-8 válido.
        let b = [b's', b'e', b's', b'i', 0xF3, b'n'];
        assert_eq!(decode(&b), Decoded::Text("sesión".into()));
    }

    /// Regresión: `\0` es UTF-8 válido. Si se valida antes de olfatear, todo
    /// LevelDB de Chromium acaba dentro del índice léxico.
    #[test]
    fn un_nulo_basta_para_declararlo_binario() {
        assert_eq!(decode(b"texto\x00mas texto"), Decoded::Binary);
        assert_eq!(
            decode(b"\x01\x00\x00\x00leveldb.BytewiseComparator"),
            Decoded::Binary
        );
    }

    #[test]
    fn un_archivo_vacio_no_es_binario() {
        assert_eq!(decode(b""), Decoded::Text(String::new()));
    }

    #[test]
    fn normalizar_deja_offsets_estables() {
        let n = normalize("a\r\n\r\n\r\n\r\nb   \r\nc");
        assert_eq!(n, "a\n\nb\nc");
        // el texto normalizado es el que se indexa: sus offsets son los reales
        assert_eq!(&n[0..1], "a");
        assert_eq!(&n[3..4], "b");
    }

    #[test]
    fn el_salto_de_pagina_del_pdf_no_ensucia_el_texto() {
        assert_eq!(
            normalize("pagina uno\u{0c}pagina dos"),
            "pagina uno pagina dos"
        );
    }

    #[test]
    fn el_hash_del_texto_ignora_el_formato_del_archivo() {
        // El mismo contenido guardado con saltos de Windows y de Unix produce
        // el mismo hash de texto, aunque los archivos difieran byte a byte.
        assert_eq!(
            text_hash(&normalize("uno\r\ndos")),
            text_hash(&normalize("uno\ndos"))
        );
    }
}
