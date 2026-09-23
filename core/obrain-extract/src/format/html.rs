//! HTML. El formato con más ruido por byte útil de todo el corpus.
//!
//! Una página guardada de 20 KB puede tener 2 KB de prosa y 18 KB de estilos,
//! scripts, menús y avisos de cookies. Indexarla entera no es "más
//! información": es un documento que sale en toda búsqueda y no responde a
//! ninguna. Por eso este extractor **quita** mucho más de lo que conserva.

use scraper::Html;
use scraper::node::Node;

/// Subárboles que nunca contienen prosa del documento.
const SKIP: &[&str] = &[
    "script", "style", "noscript", "template", "svg", "canvas", "iframe", "object", "embed", "map",
    "audio", "video", "head",
];

/// Elementos que separan bloques de texto. Sin esto, el menú de navegación y
/// el primer párrafo se pegan en una sola línea inconsultable.
///
/// Cada bloque cierra con **línea en blanco**, no con un salto simple. Es
/// deliberado: la línea en blanco es lo único que el fragmentador entiende
/// como límite de párrafo, así que un `<p>` de HTML acaba siendo un párrafo de
/// verdad y no un trozo cualquiera a 1.200 bytes del anterior.
const BLOCK: &[&str] = &[
    "p",
    "div",
    "hr",
    "li",
    "ul",
    "ol",
    "tr",
    "td",
    "th",
    "table",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "section",
    "article",
    "header",
    "footer",
    "nav",
    "aside",
    "main",
    "blockquote",
    "pre",
    "figcaption",
    "form",
    "label",
    "option",
    "dt",
    "dd",
    "details",
    "summary",
];

pub fn extract(bytes: &[u8]) -> Result<String, String> {
    let source = match crate::text::decode(bytes) {
        crate::text::Decoded::Text(t) => t,
        crate::text::Decoded::Binary => return Err("binario".to_string()),
    };
    let doc = Html::parse_document(&source);
    let mut out = String::with_capacity(source.len() / 4);

    // El `<title>` es lo único que se rescata de `<head>`, y es de lo más
    // valioso que tiene una página: casi siempre es cómo la llamaría una persona.
    if let Some(t) = doc.root_element().select_title() {
        push_text(&mut out, &t);
        push_break(&mut out);
    }
    walk(*doc.root_element(), &mut out);
    Ok(out)
}

/// Recorrido en profundidad saltando subárboles enteros.
fn walk(node: ego_tree::NodeRef<'_, Node>, out: &mut String) {
    match node.value() {
        Node::Text(t) => push_text(out, t),
        Node::Element(el) => {
            let name = el.name();
            if SKIP.contains(&name) {
                return;
            }
            let block = BLOCK.contains(&name);
            if block {
                push_break(out);
            }
            // `<br>` corta la línea pero no cierra el párrafo: una dirección
            // postal partida en tres no son tres párrafos.
            if name == "br" {
                push_line(out);
            }
            // El texto alternativo de una imagen es la única descripción que
            // existe de ella hasta que M4 mire píxeles.
            if name == "img" {
                if let Some(alt) = el.attr("alt") {
                    push_text(out, alt);
                }
            }
            for c in node.children() {
                walk(c, out);
            }
            if block {
                push_break(out);
            }
        }
        _ => {
            for c in node.children() {
                walk(c, out);
            }
        }
    }
}

/// Añade texto colapsando cualquier racha de espacios en uno solo. El sangrado
/// del HTML no es contenido.
fn push_text(out: &mut String, text: &str) {
    let mut space = out.is_empty() || out.ends_with([' ', '\n']);
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !space {
                out.push(' ');
                space = true;
            }
        } else {
            out.push(ch);
            space = false;
        }
    }
}

/// Cierra un párrafo: deja exactamente una línea en blanco.
fn push_break(out: &mut String) {
    while out.ends_with(' ') {
        out.pop();
    }
    if out.is_empty() {
        return;
    }
    while !out.ends_with("\n\n") {
        out.push('\n');
    }
}

/// Corta la línea sin cerrar el párrafo.
fn push_line(out: &mut String) {
    while out.ends_with(' ') {
        out.pop();
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

/// `<title>` sin montar un selector CSS por documento.
trait Title {
    fn select_title(&self) -> Option<String>;
}

impl Title for scraper::ElementRef<'_> {
    fn select_title(&self) -> Option<String> {
        self.descendants().find_map(|n| {
            let el = n.value().as_element()?;
            (el.name() == "title").then(|| {
                n.children()
                    .filter_map(|c| c.value().as_text().map(|t| t.to_string()))
                    .collect::<String>()
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_scripts_y_estilos_no_entran_en_el_indice() {
        let html = br#"<html><head><title>Los Frutos de Maria</title>
            <style>.hero{color:red;font-size:14px}</style></head>
            <body><script>window.dataLayer=[];gtag('js');</script>
            <h1>Crema facial</h1><p>Hecha con  aceite   de rosa mosqueta.</p></body></html>"#;
        let t = extract(html).unwrap();
        assert!(t.contains("Los Frutos de Maria"));
        assert!(t.contains("Crema facial"));
        assert!(
            t.contains("aceite de rosa mosqueta"),
            "espacios colapsados: {t:?}"
        );
        assert!(!t.contains("font-size"), "entro CSS: {t:?}");
        assert!(!t.contains("dataLayer"), "entro JS: {t:?}");
    }

    #[test]
    fn los_bloques_no_se_pegan_entre_si() {
        let t = extract(b"<body><li>Inicio</li><li>Tienda</li><p>Bienvenido</p></body>").unwrap();
        assert!(!t.contains("InicioTienda"), "{t:?}");
        // línea en blanco: cada bloque es un párrafo para el fragmentador
        assert!(t.contains("Inicio\n\nTienda"), "{t:?}");
    }

    #[test]
    fn un_br_no_parte_el_parrafo() {
        let t = extract(b"<body><p>Calle Mayor 3<br>28013 Madrid</p></body>").unwrap();
        assert!(t.contains("Calle Mayor 3\n28013 Madrid"), "{t:?}");
    }

    #[test]
    fn las_entidades_html_se_decodifican() {
        let t = extract(b"<body><p>Ni&ntilde;o &amp; se&#241;or</p></body>").unwrap();
        assert!(t.contains("Niño & señor"), "{t:?}");
    }

    #[test]
    fn el_texto_alternativo_de_una_imagen_cuenta() {
        let html = r#"<body><img src="a.webp" alt="pomada de caléndula"></body>"#;
        let t = extract(html.as_bytes()).unwrap();
        assert!(t.contains("caléndula"), "{t:?}");
    }
}
