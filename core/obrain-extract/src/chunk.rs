//! Fragmentación del texto extraído — **versionada**.
//!
//! M2 no calcula ni un embedding. Aun así el chunker lleva versión desde el
//! primer día, y no es ceremonia: el invariante I7 dice que *todo vector
//! guarda `modelo + versión + dimensión + chunker`*. Un vector sólo es
//! comparable con otro si ambos describen el mismo trozo de texto; si el
//! chunker cambia y nadie lo anotó, el índice vectorial queda envenenado sin
//! que ninguna consulta lo delate. Anotarlo ahora cuesta un entero. Retrofitearlo
//! cuando ya hay 300.000 vectores es un evento de extinción.
//!
//! # `paragraph-pack@1`
//!
//! Deliberadamente simple, porque una estrategia sofisticada que nadie puede
//! explicar es peor que una tonta que todo el mundo entiende:
//!
//! 1. El texto se parte por **líneas en blanco** — el único límite semántico
//!    que existe igual en un PDF, un Markdown y un `.docx`.
//! 2. Los párrafos se **empaquetan** hasta [`TARGET`] bytes. Un párrafo nunca
//!    se corta salvo que él solo pase de [`MAX`].
//! 3. Cada fragmento **solapa** [`OVERLAP`] bytes con el anterior, para que una
//!    frase partida por un límite siga siendo encontrable entera.
//!
//! Un fragmento **es** un rango de bytes `[start, end)` del texto normalizado,
//! y su texto es exactamente esa rebanada. Esa equivalencia no es un detalle de
//! implementación: es lo que permite que `evidence` (I9) apunte a un sitio real
//! del documento en vez de a una copia parecida.

/// Nombre de la estrategia. Va al log y a cada fila de `chunks`.
pub const CHUNKER: &str = "paragraph-pack";
/// Se incrementa **siempre** que cambie el resultado para el mismo texto.
pub const CHUNKER_VERSION: i64 = 1;

/// Tamaño al que se deja de acumular párrafos.
const TARGET: usize = 1200;
/// Un solo párrafo más largo que esto se parte por la fuerza.
const MAX: usize = 2400;
/// Cuánto se retrocede el principio de cada fragmento sobre el anterior.
const OVERLAP: usize = 200;

/// Un fragmento del texto normalizado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub ordinal: usize,
    pub byte_start: usize,
    pub byte_end: usize,
    pub text: String,
}

/// Identificador de la estrategia tal y como se guarda en el log: `nombre@version`.
pub fn chunker_id() -> String {
    format!("{CHUNKER}@{CHUNKER_VERSION}")
}

/// Fragmenta texto **ya normalizado** (ver [`crate::text::normalize`]).
pub fn chunk(text: &str) -> Vec<Chunk> {
    let spans = pack(text);
    let mut out = Vec::with_capacity(spans.len());
    let mut prev_start = 0usize;

    for (ordinal, (start, end)) in spans.into_iter().enumerate() {
        // Solape hacia atrás: se retrocede hasta OVERLAP bytes, sin invadir el
        // principio del fragmento anterior y sin partir un carácter.
        let start = if ordinal == 0 {
            start
        } else {
            let want = start.saturating_sub(OVERLAP).max(prev_start + 1);
            word_boundary(text, want.min(start), start)
        };
        prev_start = start;
        out.push(Chunk {
            ordinal,
            byte_start: start,
            byte_end: end,
            text: text[start..end].to_string(),
        });
    }
    out
}

/// Empaqueta párrafos en rangos sin solape que cubren el texto.
fn pack(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut open: Option<(usize, usize)> = None;

    for (p_start, p_end) in paragraphs(text) {
        for (s, e) in split_long(text, p_start, p_end) {
            match open {
                // el párrafo cabe en el fragmento abierto
                Some((start, _)) if e - start <= TARGET => open = Some((start, e)),
                Some((start, end)) => {
                    spans.push((start, end));
                    open = Some((s, e));
                }
                None => open = Some((s, e)),
            }
        }
    }
    if let Some(span) = open {
        spans.push(span);
    }
    spans
}

/// Rangos de los párrafos, sin las líneas en blanco que los separan.
fn paragraphs(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    let mut cursor = 0usize;

    for line in text.split_inclusive('\n') {
        let line_start = cursor;
        cursor += line.len();
        if line.trim().is_empty() {
            if let Some(s) = start.take() {
                out.push((s, line_start));
            }
        } else if start.is_none() {
            start = Some(line_start);
        }
    }
    if let Some(s) = start {
        out.push((s, cursor));
    }
    // El separador de párrafo no forma parte de ningún párrafo.
    out.into_iter()
        .map(|(s, e)| (s, s + text[s..e].trim_end().len()))
        .filter(|(s, e)| e > s)
        .collect()
}

/// Parte un párrafo que por sí solo pasa de [`MAX`]. Es el caso del PDF sin
/// saltos de línea y del `.json` de una sola línea.
fn split_long(text: &str, start: usize, end: usize) -> Vec<(usize, usize)> {
    if end - start <= MAX {
        return vec![(start, end)];
    }
    let mut out = Vec::new();
    let mut cursor = start;
    while end - cursor > MAX {
        let hard = char_boundary(text, cursor + MAX);
        let cut = word_boundary_back(text, cursor + TARGET, hard).max(cursor + 1);
        out.push((cursor, cut));
        cursor = cut;
    }
    if cursor < end {
        out.push((cursor, end));
    }
    out
}

/// Baja hasta un límite de carácter válido: rebanar un UTF-8 por la mitad es
/// un pánico, no un error.
fn char_boundary(text: &str, mut i: usize) -> usize {
    i = i.min(text.len());
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Primer hueco entre palabras en `[from, to)`; si no hay, `from` ajustado.
fn word_boundary(text: &str, from: usize, to: usize) -> usize {
    let from = char_boundary(text, from);
    let to = char_boundary(text, to);
    match text[from..to].find(char::is_whitespace) {
        Some(off) => char_boundary(text, from + off + 1).min(to),
        None => from,
    }
}

/// Último hueco entre palabras en `[from, to)`; si no hay, `to`.
fn word_boundary_back(text: &str, from: usize, to: usize) -> usize {
    let from = char_boundary(text, from);
    let to = char_boundary(text, to);
    match text[from..to].rfind(char::is_whitespace) {
        Some(off) => char_boundary(text, from + off + 1),
        None => to,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La propiedad que sostiene la evidencia (I9): el texto de un fragmento
    /// **es** la rebanada que dicen sus offsets. Si esto falla, cada cita del
    /// sistema apunta a un sitio que no es.
    fn check_invariantes(text: &str, chunks: &[Chunk]) {
        for c in chunks {
            assert_eq!(
                c.text,
                &text[c.byte_start..c.byte_end],
                "el fragmento {} no coincide con sus offsets",
                c.ordinal
            );
            assert!(c.byte_start < c.byte_end, "fragmento vacio");
        }
        for w in chunks.windows(2) {
            assert!(w[0].byte_start < w[1].byte_start, "no avanza");
            assert!(w[1].byte_start <= w[0].byte_end, "hay texto sin cubrir");
        }
        if let Some(last) = chunks.last() {
            assert_eq!(last.byte_end, text.len(), "cola de texto sin fragmentar");
        }
    }

    #[test]
    fn un_texto_corto_es_un_solo_fragmento() {
        let t = "Una nota corta.\n\nCon dos parrafos.";
        let c = chunk(t);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].text, t);
        check_invariantes(t, &c);
    }

    #[test]
    fn el_texto_vacio_no_produce_fragmentos() {
        assert!(chunk("").is_empty());
        assert!(chunk("\n\n\n").is_empty());
    }

    #[test]
    fn los_parrafos_se_empaquetan_hasta_el_objetivo() {
        let parrafo = "palabra ".repeat(60); // ~480 bytes
        let t = [parrafo.trim(); 6].join("\n\n");
        let c = chunk(&t);
        assert!(c.len() > 1, "6 parrafos de 480 bytes no caben en uno");
        for x in &c {
            assert!(
                x.byte_end - x.byte_start <= MAX,
                "fragmento de {} bytes",
                x.byte_end - x.byte_start
            );
        }
        check_invariantes(&t, &c);
    }

    #[test]
    fn un_parrafo_enorme_se_parte_por_espacios() {
        // El caso del PDF de una sola columna sin saltos: 20.000 bytes seguidos.
        let t = "lorem ".repeat(3_400);
        let t = t.trim();
        let c = chunk(t);
        assert!(c.len() > 5);
        check_invariantes(t, &c);
        // No se parte un carácter ni una palabra por la mitad.
        for x in &c {
            assert!(x.text.starts_with("lorem") || x.text.starts_with(' '));
        }
    }

    #[test]
    fn hay_solape_entre_fragmentos_consecutivos() {
        let parrafo = "palabra ".repeat(80);
        let t = [parrafo.trim(); 8].join("\n\n");
        let c = chunk(&t);
        let solapes = c
            .windows(2)
            .filter(|w| w[1].byte_start < w[0].byte_end)
            .count();
        assert_eq!(solapes, c.len() - 1, "todos los limites deben solapar");
    }

    /// Sin esto, un `.md` con acentos o un PDF en español revientan al partir.
    #[test]
    fn no_se_parte_un_caracter_multibyte() {
        let t = "áéíóú ñ € ".repeat(2_000);
        let t = t.trim();
        let c = chunk(t);
        assert!(c.len() > 3);
        check_invariantes(t, &c);
    }

    #[test]
    fn una_sola_palabra_gigantesca_no_cuelga() {
        // Un `.json` minificado sin un solo espacio.
        let t = "x".repeat(10_000);
        let c = chunk(&t);
        check_invariantes(&t, &c);
        assert!(c.len() >= 4);
    }

    #[test]
    fn el_identificador_del_chunker_va_al_log() {
        assert_eq!(chunker_id(), "paragraph-pack@1");
    }
}
