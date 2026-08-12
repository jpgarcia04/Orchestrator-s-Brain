//! Qué se enseña como prueba de una respuesta.
//!
//! I9 dice que **ninguna respuesta va sin procedencia**, y
//! [`EXPERIENCE.md`](../../../docs/EXPERIENCE.md) §4.4 exige que el «¿por qué?»
//! resalte *el fragmento exacto* del documento origen. Hasta 2026-08-12 la
//! prueba eran los primeros 160 caracteres del fragmento ganador, que es otra
//! cosa: decía **dónde empieza** el fragmento, no **por qué** está ahí.
//!
//! ## Lo que destapó auditarlo
//!
//! Sobre el corpus real, la consulta «comprobante de ingresos» —la pregunta de
//! la visa— enseñaba como prueba el sello digital del SAT en base64. Lo
//! importante es lo que la auditoría descartó de camino:
//!
//! - **La recuperación no estaba equivocada.** El fragmento ganador dice
//!   «ESTE DOCUMENTO ES UNA REPRESENTACIÓN IMPRESA DE UN CFDI», y CFDI es
//!   literalmente *Comprobante Fiscal Digital por Internet*. El modelo
//!   emparejó la clase de documento correcta por la razón correcta.
//! - **No era un problema de volumen.** Los sellos son 19 fragmentos de 4.058
//!   (0,5 % del índice) en 8 archivos. Filtrarlos del índice no habría
//!   arreglado nada de lo que se veía.
//! - **La frase que explicaba el resultado ya estaba dentro del fragmento**,
//!   al final. El recorte se quedaba con el principio.
//!
//! Por eso esto no filtra el índice: **elige la ventana**. Un sello sigue
//! estando indexado, buscable y contando para la puntuación. Lo único que no
//! hace nunca es hacerse pasar por una explicación.
//!
//! ## La señal de «esto no lo escribió una persona»
//!
//! Es hermana de la de [`crate::noise`], con la misma disciplina —señales que
//! se puedan defender de una en una— y una diferencia de escala: `noise`
//! juzga **un archivo entero**, esto juzga **un tramo dentro de un
//! documento legítimo**. Un estado de cuenta del banco es un documento real
//! que trae un sello criptográfico pegado; ningún clasificador por archivo lo
//! ve, porque el archivo no tiene nada de malo.
//!
//! La regla está medida contra el corpus real y no elegida a ojo. Se probaron
//! tres candidatas antes de quedarse con ésta:
//!
//! | Candidata | Por qué no |
//! |---|---|
//! | Tirada larga de alfabeto base64 | La extracción del PDF mete espacios y saltos **dentro** del sello. No hay tirada. |
//! | Tokens de ≥40 caracteres | Se lleva URLs y correos (`centrodeatencioneninversiones@banamex.com`), que sí son prueba legítima. 8.847 tokens del corpus. |
//! | Proporción de caracteres sin vocal | Se lleva código fuente y PDF de ejercicios de programación. |
//!
//! La que queda —[`is_machine_token`]— marca **69 tokens de 598.499** en 7
//! archivos: los sellos de los tres estados de cuenta, los de las dos líneas
//! de captura, un `.env` y siete claves de un `.ini` de configuración de un
//! motor de videojuego. De esos siete se puede discutir si son sellos; no se
//! puede discutir que los escribió una máquina.
//!
//! ## Asimetría a propósito
//!
//! Equivocarse marcando texto humano como máquina cuesta una ventana peor.
//! Equivocarse al revés enseña un sello en la pantalla que más confianza tiene
//! que generar. Así que ante la duda se descarta el tramo, y **siempre** queda
//! un plan B: si no queda nada legible, esto devuelve `None` y quien llama
//! enseña vacío. Un hueco es honesto; un sello disfrazado de explicación, no.

/// Un tramo del texto que se puede enseñar como prueba.
///
/// `text` **es** la rebanada `[byte_start, byte_end)` del fragmento, igual que
/// un [`crate::chunk::Chunk`] lo es de su documento. Esa igualdad es lo que
/// permite que la cita apunte a un sitio real del archivo y no a una copia
/// parecida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    pub byte_start: usize,
    pub byte_end: usize,
    pub text: String,
}

/// Cuánto texto se enseña. En bytes, que es la unidad de los offsets.
pub const BUDGET: usize = 240;

/// A partir de aquí un token deja de poder ser una palabra.
///
/// La palabra más larga del corpus real que no es un volcado tiene 39
/// caracteres. Por debajo de 40 la regla empieza a comerse identificadores de
/// código y palabras compuestas.
const TOKEN_LEN: usize = 40;

/// Caracteres distintos mínimos dentro del token.
///
/// Sin esto, las bandas de `=` que separan secciones en un `.css` o un `.java`
/// —60 caracteres iguales— entran como sello. Un sello base64 de 40
/// caracteres tiene 25–30 distintos; una banda tiene uno.
const TOKEN_ALPHABET: usize = 12;

/// Un tramo de prosa por debajo de esto no explica nada aunque sea legible.
/// «Sello Digital SAT» son 17 bytes y no es una respuesta.
const MIN_REGION: usize = 24;

/// Términos de la consulta más cortos que esto no se buscan.
///
/// «de», «la», «el» aparecen en todo y centrarían la ventana en cualquier
/// sitio, que es lo mismo que no centrarla.
const MIN_TERM: usize = 3;

/// ¿Este token lo escribió una máquina para una máquina?
///
/// Se juzga token a token —separado por espacios— y no por tiradas de
/// caracteres, porque los extractores de PDF parten el sello en líneas de 80
/// caracteres y una tirada continua no existe en el texto extraído.
pub fn is_machine_token(token: &str) -> bool {
    if token.len() < TOKEN_LEN {
        return false;
    }
    if !token
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=')
    {
        return false;
    }
    let distintos = token.bytes().collect::<std::collections::BTreeSet<_>>().len();
    distintos >= TOKEN_ALPHABET
}

/// ¿Es este token la **continuación** de un sello que ya está confirmado?
///
/// Un sello va partido en líneas de 80 caracteres y la última es un resto de
/// longitud arbitraria: `hFgGS2zF4X8KGm+dJP+tkg==` son 24, muy por debajo de
/// [`TOKEN_LEN`]. Medido contra el corpus real, esa cola era lo único que
/// seguía asomando en la prueba de los dos primeros resultados de la pregunta
/// de la visa después de arreglar todo lo demás.
///
/// Bajar el umbral general para cazarla habría sido el error del que avisa
/// este módulo: a 24 caracteres la regla se lleva palabras. Lo que hace válida
/// la señal aquí es la **vecindad** — sólo se pregunta por un token pegado a
/// un sello ya confirmado— más la forma, que ninguna palabra tiene: mezcla de
/// mayúsculas y minúsculas *y además* un dígito o un símbolo de base64.
/// `DOCUMENTO` no la cumple (no tiene minúsculas), `Comprobante` tampoco (no
/// tiene dígitos).
fn is_seal_continuation(token: &str) -> bool {
    if token.len() < 8 {
        return false;
    }
    if !token
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=')
    {
        return false;
    }
    token.bytes().any(|b| b.is_ascii_lowercase())
        && token.bytes().any(|b| b.is_ascii_uppercase())
        && token
            .bytes()
            .any(|b| b.is_ascii_digit() || b == b'+' || b == b'/' || b == b'=')
}

/// Los tramos de `text` que no escribió una persona, en orden y sin solaparse.
pub fn machine_spans(text: &str) -> Vec<(usize, usize)> {
    let tokens: Vec<(usize, &str)> = token_offsets(text).collect();
    let anclas: Vec<usize> = (0..tokens.len())
        .filter(|&i| is_machine_token(tokens[i].1))
        .collect();

    let mut out: Vec<(usize, usize)> = Vec::new();
    for i in anclas {
        // El sello se extiende por sus dos lados sobre lo que sólo puede ser
        // más sello: las líneas de 80 hacia atrás y la cola corta hacia
        // delante.
        let mut desde = i;
        while desde > 0 && is_seal_continuation(tokens[desde - 1].1) {
            desde -= 1;
        }
        let mut hasta = i;
        while hasta + 1 < tokens.len() && is_seal_continuation(tokens[hasta + 1].1) {
            hasta += 1;
        }
        let span = (tokens[desde].0, tokens[hasta].0 + tokens[hasta].1.len());

        // Sellos consecutivos separados sólo por espacio en blanco son un solo
        // bloque: enseñar el hueco entre dos sellos no es enseñar nada.
        match out.last_mut() {
            Some(last) if span.0 <= last.1 || text[last.1..span.0].trim().is_empty() => {
                last.1 = last.1.max(span.1);
            }
            _ => out.push(span),
        }
    }
    out
}

/// Elige qué enseñar de `text` como prueba de que responde a `query`.
///
/// Devuelve `None` cuando no queda ni un tramo legible — el caso del fragmento
/// que es sello de principio a fin.
pub fn pick(text: &str, query: &str, budget: usize) -> Option<Evidence> {
    let regions = human_regions(text);
    if regions.is_empty() {
        return None;
    }

    let (folded, map) = fold(text);
    let terms = terms(query);

    // Dónde aparece cada término, en offsets del texto original.
    let mut hits: Vec<usize> = Vec::new();
    for t in &terms {
        let mut from = 0usize;
        while let Some(off) = folded[from..].find(t.as_str()) {
            let at = from + off;
            hits.push(map[at]);
            from = at + t.len();
        }
    }

    // La ventana se centra en donde aparece lo que buscaste. Si no aparece
    // nada, se enseña el tramo de prosa más largo, que es el que más dice.
    let mejor = hits
        .iter()
        .filter_map(|&at| region_of(&regions, at).map(|r| (at, r)))
        .map(|(at, r)| {
            let span = window(text, r, at, budget);
            (score(&folded, &map, &terms, span), span)
        })
        .max_by_key(|&(sc, (s, e))| (sc, e - s, std::cmp::Reverse(s)));

    let (start, end) = match mejor {
        Some((sc, span)) if sc > 0 => span,
        _ => {
            let r = *regions.iter().max_by_key(|(s, e)| e - s)?;
            window(text, r, r.0, budget)
        }
    };

    Some(Evidence {
        byte_start: start,
        byte_end: end,
        text: text[start..end].to_string(),
    })
}

/// Los tramos legibles: lo que queda al quitar los sellos, recortado y sin las
/// migajas que quedan entre dos sellos.
fn human_regions(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    for (s, e) in machine_spans(text).into_iter().chain([(text.len(), text.len())]) {
        if s > cursor {
            if let Some(r) = trim(text, cursor, s) {
                if r.1 - r.0 >= MIN_REGION {
                    out.push(r);
                }
            }
        }
        cursor = e;
    }
    out
}

/// Recorta el espacio en blanco de los bordes de un tramo.
fn trim(text: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let slice = &text[start..end];
    let delante = slice.len() - slice.trim_start().len();
    let detras = slice.len() - slice.trim_end().len();
    let (s, e) = (start + delante, end - detras);
    (e > s).then_some((s, e))
}

/// La ventana de `budget` bytes dentro de `region` que rodea a `at`.
///
/// Se deja un tercio por delante: la frase que explica algo casi siempre sigue
/// a la palabra buscada, no la precede.
fn window(text: &str, region: (usize, usize), at: usize, budget: usize) -> (usize, usize) {
    let (rs, re) = region;
    if re - rs <= budget {
        return (rs, re);
    }
    let quiero = at.saturating_sub(budget / 3).max(rs);
    let start = quiero.min(re - budget);
    let start = word_start(text, start, rs);
    let end = word_end(text, (start + budget).min(re), re);
    (start, end)
}

/// Cuántos términos distintos caen dentro de la ventana.
fn score(folded: &str, map: &[usize], terms: &[String], span: (usize, usize)) -> usize {
    terms
        .iter()
        .filter(|t| {
            folded.match_indices(t.as_str()).any(|(i, _)| {
                let at = map[i];
                at >= span.0 && at < span.1
            })
        })
        .count()
}

fn region_of(regions: &[(usize, usize)], at: usize) -> Option<(usize, usize)> {
    regions.iter().copied().find(|&(s, e)| at >= s && at < e)
}

/// Retrocede hasta el principio de la palabra, sin salir de `floor`.
fn word_start(text: &str, mut i: usize, floor: usize) -> usize {
    i = char_boundary(text, i.max(floor));
    while i > floor && !text[..i].ends_with(char::is_whitespace) {
        i = char_boundary(text, i - 1);
    }
    i
}

/// Avanza hasta el final de la palabra, sin pasar de `ceil`.
fn word_end(text: &str, mut i: usize, ceil: usize) -> usize {
    i = char_boundary(text, i.min(ceil));
    while i < ceil && !text[i..].starts_with(char::is_whitespace) {
        i = next_boundary(text, i);
    }
    i
}

fn char_boundary(text: &str, mut i: usize) -> usize {
    i = i.min(text.len());
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn next_boundary(text: &str, mut i: usize) -> usize {
    i = (i + 1).min(text.len());
    while i < text.len() && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Los tokens del texto con su offset de byte.
fn token_offsets(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.split_whitespace()
        .map(move |t| (t.as_ptr() as usize - text.as_ptr() as usize, t))
}

/// Términos buscables de una consulta: minúsculas, sin diacríticos, sin
/// palabras vacías por cortas, sin repetidos.
fn terms(query: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for palabra in query.split(|c: char| !c.is_alphanumeric()) {
        let (t, _) = fold(palabra);
        if t.len() >= MIN_TERM && !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

/// Minúsculas y sin diacríticos, **conservando la correspondencia de
/// offsets**: `map[i]` es el byte de `text` del que salió el byte `i`.
///
/// Sin el mapa, buscar sobre el texto plegado devolvería posiciones que no
/// existen en el original y la cita apuntaría a un sitio equivocado — que es
/// exactamente el fallo del que este módulo viene.
fn fold(text: &str) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(text.len());
    let mut map = Vec::with_capacity(text.len());
    for (offset, ch) in text.char_indices() {
        let plano = match ch {
            'á' | 'à' | 'ä' | 'â' | 'Á' | 'À' | 'Ä' | 'Â' => 'a',
            'é' | 'è' | 'ë' | 'ê' | 'É' | 'È' | 'Ë' | 'Ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' | 'Í' | 'Ì' | 'Ï' | 'Î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'Ó' | 'Ò' | 'Ö' | 'Ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' | 'Ú' | 'Ù' | 'Ü' | 'Û' => 'u',
            'ñ' | 'Ñ' => 'n',
            'ç' | 'Ç' => 'c',
            c => c.to_ascii_lowercase(),
        };
        // Un carácter plegado puede seguir ocupando varios bytes (los que no
        // son latinos no se tocan); todos apuntan al mismo origen.
        let antes = out.len();
        out.push(plano);
        map.resize(out.len(), offset);
        debug_assert!(map.len() > antes);
    }
    (out, map)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La propiedad que sostiene I9: lo que se enseña **es** la rebanada que
    /// dicen sus offsets. Si esto falla, cada cita del sistema apunta a un
    /// sitio que no es.
    fn check(text: &str, ev: &Evidence) {
        assert_eq!(ev.text, &text[ev.byte_start..ev.byte_end]);
        assert!(ev.byte_start < ev.byte_end);
    }

    // ── la señal ──

    #[test]
    fn un_sello_del_sat_es_texto_de_maquina() {
        let sello = "cVejnMn0metP6NKFVY3yqH6kpl9/a2ShlcblLAgmESH+55HuX8nTdiyGC0vsy9bzLpz6534S92lDEAM2";
        assert!(is_machine_token(sello));
    }

    /// Regresión del corpus real: la regla de «token largo» se llevaba esto,
    /// y un correo dentro de un documento es prueba legítima.
    #[test]
    fn un_correo_largo_no_es_texto_de_maquina() {
        assert!(!is_machine_token("centrodeatencioneninversiones@banamex.com"));
    }

    #[test]
    fn una_url_no_es_texto_de_maquina() {
        assert!(!is_machine_token(
            "https://www.banamex.com/bancapatrimonial/inicio.html"
        ));
    }

    /// Regresión: las bandas de `=` que separan secciones en un `.css` o un
    /// `.java` son 60 caracteres del alfabeto base64 y un solo carácter
    /// distinto.
    #[test]
    fn una_banda_de_separacion_no_es_un_sello() {
        assert!(!is_machine_token(&"=".repeat(64)));
        assert!(!is_machine_token(&"abab".repeat(20)));
    }

    #[test]
    fn una_palabra_larga_normal_no_es_texto_de_maquina() {
        assert!(!is_machine_token("esternocleidomastoideo"));
        assert!(!is_machine_token("ConcurrentModificationException"));
    }

    /// Regresión del corpus real: tras arreglar todo lo demás, lo único que
    /// seguía asomando en la prueba de la pregunta de la visa eran los 24
    /// caracteres del final del sello.
    #[test]
    fn la_cola_corta_de_un_sello_va_con_el_sello() {
        let largo = "iwX4NqLgLpsOJX36FLppM+lpBwsv2SodqahcWZz3FWTiHRJSEMgNsHWOqAAeLr9ePvNDGJAlMHDXx+pY";
        let cola = "hFgGS2zF4X8KGm+dJP+tkg==";
        assert!(!is_machine_token(cola), "por sí sola es demasiado corta");
        let texto = format!("{largo}\n{cola}\n\nESTE DOCUMENTO ES UNA REPRESENTACIÓN IMPRESA DE UN CFDI");
        let ev = pick(&texto, "comprobante", BUDGET).unwrap();
        check(&texto, &ev);
        assert!(!ev.text.contains("hFgGS2"), "enseñó la cola: {:?}", ev.text);
        assert!(ev.text.contains("CFDI"));
    }

    /// La vecindad es lo que hace válida la regla de la cola. Sin un sello al
    /// lado, estas palabras no se tocan.
    #[test]
    fn una_palabra_normal_no_es_continuacion_de_nada() {
        assert!(!is_seal_continuation("DOCUMENTO"));
        assert!(!is_seal_continuation("Comprobante"));
        assert!(!is_seal_continuation("REPRESENTACION"));
        assert!(!is_seal_continuation("Fiscal"));
    }

    #[test]
    fn los_sellos_seguidos_se_juntan_en_un_solo_tramo() {
        let s = "Bu6YkTnIjkqhfh+tysHTfaddjFwPMiLfYjtTq+WncDXWfz5hPq9kBiAOvR157Vgz";
        assert!(is_machine_token(s));
        let texto = format!("{s}\n{s}");
        assert_eq!(machine_spans(&texto).len(), 1, "dos sellos seguidos son uno");
    }

    // ── la elección de ventana ──

    /// El caso que motivó el módulo, reconstruido con la forma real del
    /// fragmento 82 del corpus: sello delante, la frase que explica detrás.
    #[test]
    fn la_prueba_salta_el_sello_y_enseña_la_frase_que_explica() {
        let sello = "iwX4NqLgLpsOJX36FLppM+lpBwsv2SodqahcWZz3FWTiHRJSEMgNsHWOqAAeLr9ePvNDGJAlMHDXx+pY";
        let texto = format!(
            "{sello}\n{sello}\n\nESTE DOCUMENTO ES UNA REPRESENTACIÓN IMPRESA DE UN CFDI\n\n\
             Folio Fiscal: 7E99BB34-A9F0-42FA-AD06-D818CC775D5A"
        );
        let ev = pick(&texto, "comprobante fiscal", BUDGET).expect("hay texto legible");
        check(&texto, &ev);
        assert!(ev.text.contains("CFDI"), "enseñó {:?}", ev.text);
        assert!(!ev.text.contains("iwX4Nq"), "enseñó el sello: {:?}", ev.text);
    }

    #[test]
    fn la_ventana_se_centra_en_lo_que_buscaste() {
        let relleno = "palabra ".repeat(80);
        let texto = format!("{relleno}el aguacate maduro {relleno}");
        let ev = pick(&texto, "aguacate", BUDGET).unwrap();
        check(&texto, &ev);
        assert!(ev.text.contains("aguacate"));
    }

    #[test]
    fn los_acentos_no_impiden_encontrar_el_termino() {
        let relleno = "relleno ".repeat(60);
        let texto = format!("{relleno}la matrícula del año {relleno}");
        let ev = pick(&texto, "MATRICULA", BUDGET).unwrap();
        check(&texto, &ev);
        assert!(ev.text.contains("matrícula"), "enseñó {:?}", ev.text);
    }

    /// Sin coincidencia no se devuelve el principio por inercia: se devuelve
    /// el tramo de prosa más largo, que es el que más dice.
    #[test]
    fn sin_coincidencia_se_enseña_el_tramo_mas_largo() {
        let sello = "Zm9vYmFyYmF6cXV4cXV1eDEyMzQ1Njc4OTBhYmNkZWY".to_string();
        assert!(is_machine_token(&sello));
        let texto = format!("{sello}\n\nUna frase larga y perfectamente legible que explica el documento entero.");
        let ev = pick(&texto, "nada que ver", BUDGET).unwrap();
        check(&texto, &ev);
        assert!(ev.text.starts_with("Una frase larga"));
    }

    /// Un fragmento que es sello de principio a fin no tiene prueba que
    /// enseñar, y decirlo es mejor que enseñar el sello.
    #[test]
    fn un_fragmento_que_es_todo_sello_no_tiene_prueba() {
        let sello = "iwX4NqLgLpsOJX36FLppM+lpBwsv2SodqahcWZz3FWTiHRJSEMgNsHWOqAAeLr9ePvNDGJAlMHDXx+pY";
        let texto = format!("{sello}\n{sello}\n{sello}");
        assert_eq!(pick(&texto, "comprobante", BUDGET), None);
    }

    #[test]
    fn un_texto_corto_se_enseña_entero() {
        let t = "Resolución del IMSS de pensión.";
        let ev = pick(t, "pension", BUDGET).unwrap();
        check(t, &ev);
        assert_eq!(ev.text, t);
    }

    #[test]
    fn el_texto_vacio_no_tiene_prueba() {
        assert_eq!(pick("", "algo", BUDGET), None);
        assert_eq!(pick("   \n\n ", "algo", BUDGET), None);
    }

    /// Rebanar un UTF-8 por la mitad es un pánico, no un resultado peor.
    #[test]
    fn no_se_parte_un_caracter_multibyte() {
        let t = "áéíóú ñ € ".repeat(120);
        for q in ["ñ", "euro", "aeiou", ""] {
            let ev = pick(&t, q, BUDGET).unwrap();
            check(&t, &ev);
        }
    }

    #[test]
    fn la_ventana_respeta_el_presupuesto() {
        let t = "palabra ".repeat(300);
        let ev = pick(&t, "palabra", BUDGET).unwrap();
        check(&t, &ev);
        // Se permite pasarse hasta acabar la palabra, no más.
        assert!(ev.text.len() <= BUDGET + 16, "{} bytes", ev.text.len());
    }

    #[test]
    fn el_plegado_conserva_los_offsets() {
        let t = "camión Ñandú café";
        let (f, map) = fold(t);
        assert_eq!(f, "camion nandu cafe");
        for (i, _) in f.char_indices() {
            assert!(t.is_char_boundary(map[i]), "offset {i} no es válido");
        }
        // La 'ó' de camión ocupa 2 bytes en el original y 1 en el plegado.
        let at = f.find("camion").unwrap();
        assert_eq!(map[at], t.find("camión").unwrap());
    }
}
