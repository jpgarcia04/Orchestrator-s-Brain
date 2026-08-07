//! `docx`, `xlsx`, `pptx` — un ZIP con XML dentro.
//!
//! No hace falta ninguna librería de Office: los tres formatos guardan su
//! texto visible en elementos `<t>` (`w:t` en Word, `a:t` en PowerPoint, `t`
//! en la tabla de cadenas de Excel). Un lector de ZIP y uno de XML bastan.
//!
//! **Lo que este extractor no hace, y por qué:** de un `xlsx` sólo saca la
//! tabla de cadenas compartidas, no las fórmulas ni los números celda a celda.
//! Lo que hace encontrable una hoja de cálculo son sus rótulos —"Presupuesto
//! Universidad Canadá", "CFE consumos electricidad"—, no que en G14 haya un
//! 1.284,50. Resolver referencias de celda es trabajo real a cambio de ruido.
//!
//! **Advertencia sobre este corpus:** los 14 documentos de Office de esta
//! máquina están *todos* deshidratados en OneDrive, así que este extractor
//! jamás llega a ejecutarse sobre ellos. Está probado con material sintético.
//! Mismo estado que `RECALL_ON_OPEN` en M1: implementado, no verificado
//! contra la realidad. El informe de M2 lo dice igual de claro.

use std::io::{Cursor, Read};

/// Elementos cuyo contenido es texto visible, por nombre local (sin `w:`/`a:`).
const TEXT_TAGS: &[&str] = &["t"];
/// Elementos que cierran un párrafo: `w:p` (Word), `a:p` (PowerPoint),
/// `si` (cada cadena compartida de Excel).
const BREAK_TAGS: &[&str] = &["p", "si", "br"];

pub fn extract(bytes: &[u8]) -> Result<String, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("zip: {e}"))?;

    let parts: Vec<String> = zip
        .file_names()
        .filter(|n| is_content_part(n))
        .map(str::to_string)
        .collect();
    if parts.is_empty() {
        return Err("ooxml: sin partes de contenido".to_string());
    }

    // Orden estable: `slide2.xml` antes que `slide10.xml`, y el documento
    // antes que sus notas. Sin esto, el mismo archivo produce texto distinto
    // en cada ejecución y el hash del texto deja de significar nada.
    let mut parts = parts;
    parts.sort_by_key(|n| (part_rank(n), natural_key(n)));

    let mut out = String::new();
    for name in parts {
        let mut buf = Vec::new();
        zip.by_name(&name)
            .map_err(|e| format!("zip: {name}: {e}"))?
            .read_to_end(&mut buf)
            .map_err(|e| format!("zip: {name}: {e}"))?;
        let text = text_from_xml(&buf)?;
        if !text.trim().is_empty() {
            out.push_str(&text);
            out.push_str("\n\n");
        }
    }
    Ok(out)
}

fn is_content_part(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n == "word/document.xml"
        || n == "word/footnotes.xml"
        || n == "word/endnotes.xml"
        || n == "xl/sharedstrings.xml"
        || (n.starts_with("ppt/slides/slide") && n.ends_with(".xml"))
        || (n.starts_with("ppt/notesslides/notesslide") && n.ends_with(".xml"))
}

fn part_rank(name: &str) -> u8 {
    let n = name.to_ascii_lowercase();
    match () {
        _ if n == "word/document.xml" => 0,
        _ if n.starts_with("ppt/slides/") => 1,
        _ if n == "xl/sharedstrings.xml" => 2,
        _ => 3,
    }
}

/// `slide9` < `slide10`: comparar como texto los ordenaría al revés.
fn natural_key(name: &str) -> (String, u32) {
    let digits: String = name.chars().filter(char::is_ascii_digit).collect();
    let prefix: String = name.chars().filter(|c| !c.is_ascii_digit()).collect();
    (prefix, digits.parse().unwrap_or(0))
}

/// Saca el texto visible de una parte XML de OOXML.
fn text_from_xml(xml: &[u8]) -> Result<String, String> {
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut depth_in_text = 0usize;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(e)) => {
                let name = e.name();
                if TEXT_TAGS.contains(&local_name(name.as_ref())) {
                    depth_in_text += 1;
                }
            }
            Ok(quick_xml::events::Event::End(e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                if TEXT_TAGS.contains(&local) {
                    depth_in_text = depth_in_text.saturating_sub(1);
                } else if BREAK_TAGS.contains(&local) {
                    push_break(&mut out);
                }
            }
            Ok(quick_xml::events::Event::Empty(e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                if local == "br" || local == "cr" {
                    push_break(&mut out);
                } else if local == "tab" {
                    out.push('\t');
                }
            }
            Ok(quick_xml::events::Event::Text(e)) if depth_in_text > 0 => {
                let decoded = e.decode().map_err(|err| format!("xml: {err}"))?;
                out.push_str(decoded.as_ref());
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(format!("xml: {e}")),
        }
        buf.clear();
    }
    Ok(out)
}

fn local_name(qname: &[u8]) -> &str {
    let s = std::str::from_utf8(qname).unwrap_or("");
    s.rsplit(':').next().unwrap_or(s)
}

fn push_break(out: &mut String) {
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Construye un OOXML mínimo pero real: ZIP de verdad, XML de verdad.
    fn zip_with(parts: &[(&str, &str)]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut w = zip::ZipWriter::new(Cursor::new(&mut buf));
            let opts: zip::write::FileOptions<'_, ()> =
                zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            for (name, body) in parts {
                w.start_file(*name, opts).unwrap();
                w.write_all(body.as_bytes()).unwrap();
            }
            w.finish().unwrap();
        }
        buf
    }

    #[test]
    fn un_docx_da_sus_parrafos() {
        let doc = r#"<?xml version="1.0"?>
            <w:document xmlns:w="x"><w:body>
              <w:p><w:r><w:t>Estimado Sr. Garc</w:t><w:t>ía</w:t></w:r></w:p>
              <w:p><w:r><w:t xml:space="preserve">Adjunto la revisión del almacén.</w:t></w:r></w:p>
            </w:body></w:document>"#;
        let z = zip_with(&[("word/document.xml", doc)]);
        let t = extract(&z).unwrap();
        assert!(t.contains("Estimado Sr. García"), "{t:?}");
        assert!(t.contains("Adjunto la revisión del almacén."), "{t:?}");
        assert!(!t.contains("w:t"), "entro markup: {t:?}");
    }

    #[test]
    fn de_un_xlsx_salen_los_rotulos_no_los_numeros() {
        let shared = r#"<sst xmlns="x">
            <si><t>Presupuesto Universidad Canadá</t></si>
            <si><t>CFE Consumos Electricidad</t></si></sst>"#;
        let sheet = r#"<worksheet><sheetData><row><c r="A1"><v>1284.5</v></c></row></sheetData></worksheet>"#;
        let z = zip_with(&[
            ("xl/sharedStrings.xml", shared),
            ("xl/worksheets/sheet1.xml", sheet),
        ]);
        let t = extract(&z).unwrap();
        assert!(t.contains("Presupuesto Universidad Canadá"), "{t:?}");
        assert!(t.contains("CFE Consumos Electricidad"), "{t:?}");
        assert!(!t.contains("1284.5"), "los numeros sueltos son ruido: {t:?}");
    }

    #[test]
    fn las_diapositivas_salen_en_orden_natural() {
        let slide = |n: &str| format!(r#"<p:sld xmlns:a="x"><a:p><a:t>{n}</a:t></a:p></p:sld>"#);
        let (s2, s10) = (slide("segunda"), slide("decima"));
        let z = zip_with(&[
            ("ppt/slides/slide10.xml", s10.as_str()),
            ("ppt/slides/slide2.xml", s2.as_str()),
        ]);
        let t = extract(&z).unwrap();
        assert!(
            t.find("segunda") < t.find("decima"),
            "slide2 va antes que slide10: {t:?}"
        );
    }

    #[test]
    fn un_zip_que_no_es_office_no_revienta() {
        let z = zip_with(&[("hola.txt", "no soy office")]);
        assert!(extract(&z).is_err());
        assert!(extract(b"PK\x03\x04 basura").is_err());
    }
}
