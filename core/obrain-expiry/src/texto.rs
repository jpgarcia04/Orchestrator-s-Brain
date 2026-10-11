//! Encontrar vencimientos en un texto: **una palabra de caducidad y una
//! fecha**, en ese orden y juntas.
//!
//! Un documento está lleno de fechas —nacimiento, expedición, firma, pago— y
//! casi ninguna es un vencimiento. Por eso esto no busca fechas: busca la
//! palabra que dice qué es la fecha que viene detrás. Las reglas, su peso y
//! por qué «hasta» suelto no basta están en
//! [ADR-0023](../../../docs/adr/0023-vencimientos.md).
//!
//! Todo se busca sobre una copia **plegada** del texto —minúsculas y sin
//! acentos— que recuerda de qué byte del original sale cada byte suyo. Así
//! «VALIDEZ», «Validez» y «válida hasta» se leen igual, y la evidencia sigue
//! apuntando al sitio exacto del documento.
//!
//! Las dos lecturas que **no** necesitan palabra —dos fechas a N años justos en
//! un documento de identidad, y la zona MRZ— viven en [`crate::plazo`] y
//! [`crate::mrz`] ([ADR-0028](../../../docs/adr/0028-vencimientos-sin-palabra.md)).

use crate::fecha::Fecha;
use regex::Regex;
use std::sync::LazyLock;

/// Con qué palabra se leyó una fecha. Es la mitad del «¿por qué?».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Regla {
    /// caduca, caducidad, expira, date of expiry…
    Caducidad,
    /// validez, válido hasta, valid until…
    Validez,
    /// vence, vencimiento. También se usa para pagos: pesa igual y se fía menos.
    Vencimiento,
    /// vigencia, vigente hasta.
    Vigencia,
    /// «hasta» suelto, sólo con contexto de vigencia cerca.
    Hasta,
    /// Sin palabra: dos fechas a N años justos en un documento de identidad,
    /// la expedición y la caducidad (ADR-0028).
    Plazo,
    /// Sin palabra: la zona de lectura mecánica (MRZ) de un pasaporte o de una
    /// tarjeta, con su dígito de control (ADR-0028).
    Mrz,
}

impl Regla {
    pub fn as_str(self) -> &'static str {
        match self {
            Regla::Caducidad => "caducidad",
            Regla::Validez => "validez",
            Regla::Vencimiento => "vencimiento",
            Regla::Vigencia => "vigencia",
            Regla::Hasta => "hasta",
            Regla::Plazo => "plazo",
            Regla::Mrz => "mrz",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "caducidad" => Regla::Caducidad,
            "validez" => Regla::Validez,
            "vencimiento" => Regla::Vencimiento,
            "vigencia" => Regla::Vigencia,
            "hasta" => Regla::Hasta,
            "plazo" => Regla::Plazo,
            "mrz" => Regla::Mrz,
            _ => return None,
        })
    }

    /// Cuánto dice la palabra de que la fecha es **el** vencimiento del
    /// documento. Decide entre varias candidatas antes que la fecha.
    ///
    /// La MRZ pesa más que cualquier palabra: lleva dígito de control, así que
    /// un número mal leído por el OCR se descarta en vez de dar otra fecha. El
    /// plazo pesa lo mínimo: sólo decide si ninguna palabra dijo nada.
    pub fn peso(self) -> u8 {
        match self {
            Regla::Mrz => 4,
            Regla::Caducidad | Regla::Validez | Regla::Vencimiento => 3,
            Regla::Vigencia => 2,
            Regla::Hasta | Regla::Plazo => 1,
        }
    }

    /// La confianza de la aserción inferida.
    pub fn confianza(self) -> f64 {
        match self {
            Regla::Mrz => 0.95,
            Regla::Caducidad | Regla::Validez => 0.85,
            Regla::Vencimiento => 0.75,
            Regla::Vigencia | Regla::Plazo => 0.7,
            Regla::Hasta => 0.55,
        }
    }
}

/// Una fecha que podría ser el vencimiento, con dónde está.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidato {
    pub fecha: Fecha,
    pub regla: Regla,
    /// `[inicio, fin)` en bytes del texto original: de la palabra al final de
    /// la fecha. Es lo que se resalta al pedir «¿por qué?».
    pub inicio: usize,
    pub fin: usize,
    /// Con la regla `plazo`, cuántos años justos hay desde la otra fecha. Es
    /// la otra mitad de su «¿por qué?».
    pub anios: Option<u8>,
}

// ─────────────────────────────────────────────────────────────── plegar

/// El texto en minúsculas y sin acentos, con el mapa de vuelta al original.
pub(crate) struct Plegado {
    pub(crate) texto: String,
    /// `origen[i]` es el byte del original del que sale el byte `i` de
    /// `texto`. Lleva un elemento más, con la longitud del original, para
    /// poder traducir el final de un tramo.
    pub(crate) origen: Vec<usize>,
}

pub(crate) fn plegar(original: &str) -> Plegado {
    let mut texto = String::with_capacity(original.len());
    let mut origen = Vec::with_capacity(original.len() + 1);
    for (i, c) in original.char_indices() {
        for l in c.to_lowercase() {
            let l = match l {
                'á' | 'à' | 'â' | 'ä' | 'ã' => 'a',
                'é' | 'è' | 'ê' | 'ë' => 'e',
                'í' | 'ì' | 'î' | 'ï' => 'i',
                'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
                'ú' | 'ù' | 'û' | 'ü' => 'u',
                'ñ' => 'n',
                'ç' => 'c',
                otra => otra,
            };
            texto.push(l);
            origen.resize(texto.len(), i);
        }
    }
    origen.push(original.len());
    Plegado { texto, origen }
}

// ─────────────────────────────────────────────────────────────── palabras

/// Las palabras de caducidad, ya plegadas. El orden dentro de cada grupo va
/// de la más larga a la más corta, porque la alternancia se queda con la
/// primera que encaja.
static PALABRA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)\b(?:
          (?P<caducidad>fecha\s+de\s+caducidad|caducidad|caducad[oa]s?|caducara|caducan?
            |fecha\s+de\s+expiracion|expiracion|expirad[oa]|expirara|expiran?
            |date\s+of\s+expiry|expiry\s+date|expiration\s+date|expiry|expires)
         |(?P<validez>valid[oa]s?\s+hasta|validez|valid\s+until|valid\s+thru)
         |(?P<vencimiento>fecha\s+de\s+vencimiento|vencimiento|vencid[oa]|vencera|vencen?)
         |(?P<vigencia>vigente\s+hasta|vigencia)
         |(?P<hasta>hasta)
        )\b",
    )
    .expect("la expresión de las palabras de caducidad es válida")
});

/// Palabras que pueden ir entre la palabra de caducidad y la fecha sin
/// cambiar lo que significa: «vence **el día** 30…», «validez**:** 15 11 2026».
const RELLENO: &[&str] = &[
    "el", "la", "los", "las", "lo", "del", "de", "dia", "en", "su", "es", "sera", "a", "partir",
    "fecha", "date", "the", "on", "of", "is", "this",
];

/// Palabras que dicen que la fecha que sigue **no** es un vencimiento.
const FRENO: &[&str] = &[
    "expedicion",
    "expedido",
    "expedida",
    "emision",
    "emitido",
    "emitida",
    "nacimiento",
    "alta",
    "inicio",
    "firma",
    "issue",
    "birth",
];

/// Palabras libres (ni relleno ni freno) que se toleran en medio:
/// «validez **del permiso**: 15/11/2026».
const LIBRES: usize = 2;

/// Lo más lejos que puede quedar la fecha de su palabra, en bytes plegados.
const HUECO: usize = 48;

/// Qué tiene que haber cerca de un «hasta» suelto para que cuente.
const CONTEXTO_HASTA: &[&str] = &[
    "vigencia",
    "vigente",
    "cobertura",
    "periodo",
    "contrato",
    "poliza",
    "seguro",
    "desde",
    "valid",
    "duracion",
];

/// Cuánto se mira hacia atrás buscando ese contexto, en bytes plegados.
const VENTANA_HASTA: usize = 100;

// ─────────────────────────────────────────────────────────────── fechas

const MESES: &str = "enero|febrero|marzo|abril|mayo|junio|julio|agosto|septiembre|setiembre|octubre|noviembre|diciembre";
const ABREV: &str = "ene|jan|feb|mar|abr|apr|may|jun|jul|ago|aug|sept|sep|set|oct|nov|dic|dec";

fn mes(nombre: &str) -> Option<u32> {
    let n = match nombre {
        "enero" | "ene" | "jan" => 1,
        "febrero" | "feb" => 2,
        "marzo" | "mar" => 3,
        "abril" | "abr" | "apr" => 4,
        "mayo" | "may" => 5,
        "junio" | "jun" => 6,
        "julio" | "jul" => 7,
        "agosto" | "ago" | "aug" => 8,
        "septiembre" | "setiembre" | "sept" | "sep" | "set" => 9,
        "octubre" | "oct" => 10,
        "noviembre" | "nov" => 11,
        "diciembre" | "dic" | "dec" => 12,
        _ => return None,
    };
    Some(n)
}

/// Un año de dos o de cuatro cifras. Dos cifras son del siglo XXI.
fn anio(s: &str) -> Option<i32> {
    let n: i32 = s.parse().ok()?;
    Some(if s.len() == 2 { 2000 + n } else { n })
}

fn re(patron: &str) -> Regex {
    Regex::new(patron).expect("la expresión de fecha es válida")
}

/// Las formas de escribir una fecha, en el orden en que se prueban. Todas
/// anclan al principio: la fecha empieza donde acaba el hueco.
static ISO: LazyLock<Regex> = LazyLock::new(|| re(r"^(\d{4})-(\d{1,2})-(\d{1,2})\b"));
/// «31 de agosto de 2027», «31 agosto 2027».
static LARGA: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r"^(\d{{1,2}})(?:\s+de)?\s+({MESES})(?:\s+del?)?\s+(\d{{4}})\b"
    ))
});
/// «12 MAR 2031», «15 NOV/NOV 2026», «12-mar-31».
static ABREVIADA: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r"^(\d{{1,2}})[\s.\-/]*({ABREV})\.?(?:\s*/\s*(?:{ABREV})\.?)?[\s.\-/]*(\d{{4}}|\d{{2}})\b"
    ))
});
/// «15/11/2026», «15-11-26», «15.11.2026».
static NUMERICA: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(\d{1,2})[/.\-](\d{1,2})[/.\-](\d{4}|\d{2})\b"));
/// «15 11 2026»: así sale del OCR la validez de una TIE.
static ESPACIADA: LazyLock<Regex> = LazyLock::new(|| re(r"^(\d{1,2})\s+(\d{1,2})\s+(\d{4})\b"));
/// «12/2028».
static MES_ANIO: LazyLock<Regex> = LazyLock::new(|| re(r"^(\d{1,2})[/.\-](\d{4})\b"));
/// «12/28», como en las tarjetas. Sólo si el año no puede ser un mes.
static MES_AA: LazyLock<Regex> = LazyLock::new(|| re(r"^(\d{1,2})/(\d{2})\b"));
/// «agosto de 2027».
static NOMBRE_ANIO: LazyLock<Regex> =
    LazyLock::new(|| re(&format!(r"^({MESES})(?:\s+del?)?\s+(\d{{4}})\b")));

/// La fecha **con día** con la que empieza `s` (plegado), si empieza con una,
/// y cuántos bytes ocupa. Son las únicas que sirven para medir un plazo.
pub(crate) fn fecha_completa_al_inicio(s: &str) -> Option<(Fecha, usize)> {
    let n = |c: &regex::Captures<'_>, i: usize| c[i].parse::<u32>().ok();
    if let Some(c) = ISO.captures(s) {
        let f = Fecha::new(c[1].parse().ok()?, n(&c, 2)?, n(&c, 3)?)?;
        return Some((f, c[0].len()));
    }
    if let Some(c) = LARGA.captures(s) {
        let f = Fecha::new(anio(&c[3])?, mes(&c[2])?, n(&c, 1)?)?;
        return Some((f, c[0].len()));
    }
    if let Some(c) = ABREVIADA.captures(s) {
        let f = Fecha::new(anio(&c[3])?, mes(&c[2])?, n(&c, 1)?)?;
        return Some((f, c[0].len()));
    }
    if let Some(c) = NUMERICA.captures(s) {
        // Siempre día/mes: son documentos de España y de México.
        let f = Fecha::new(anio(&c[3])?, n(&c, 2)?, n(&c, 1)?)?;
        return Some((f, c[0].len()));
    }
    if let Some(c) = ESPACIADA.captures(s) {
        let f = Fecha::new(anio(&c[3])?, n(&c, 2)?, n(&c, 1)?)?;
        return Some((f, c[0].len()));
    }
    None
}

/// La fecha con la que empieza `s`, si empieza con una, y cuántos bytes ocupa.
///
/// Con `anio_suelto`, también un año solo —«VIGENCIA 2033», o «2023 - 2033»
/// en la INE—, que vale hasta el 31 de diciembre de ese año.
fn fecha_al_inicio(s: &str, anio_suelto: bool) -> Option<(Fecha, usize)> {
    let n = |c: &regex::Captures<'_>, i: usize| c[i].parse::<u32>().ok();
    if let Some(r) = fecha_completa_al_inicio(s) {
        return Some(r);
    }
    // Tenía forma de fecha con día y no existe («11/31/2027»): no es otra
    // cosa más corta («11/31» como mes y año).
    if [&ISO, &LARGA, &ABREVIADA, &NUMERICA, &ESPACIADA]
        .iter()
        .any(|r| r.is_match(s))
    {
        return None;
    }
    if let Some(c) = MES_ANIO.captures(s) {
        let f = Fecha::fin_de_mes(anio(&c[2])?, n(&c, 1)?)?;
        return Some((f, c[0].len()));
    }
    if let Some(c) = MES_AA.captures(s) {
        // «05/12» puede ser el 5 de diciembre sin año; «12/28» no.
        let aa = n(&c, 2)?;
        if aa > 12 {
            let f = Fecha::fin_de_mes(2000 + aa as i32, n(&c, 1)?)?;
            return Some((f, c[0].len()));
        }
        return None;
    }
    if let Some(c) = NOMBRE_ANIO.captures(s) {
        let f = Fecha::fin_de_mes(anio(&c[2])?, mes(&c[1])?)?;
        return Some((f, c[0].len()));
    }
    if anio_suelto {
        if let Some(c) = ANIO.captures(s) {
            let largo = c[0].len();
            if !es_un_anio_solo(&s[largo..]) {
                return None;
            }
            let f = Fecha::new(c[1].parse().ok()?, 12, 31)?;
            return Some((f, largo));
        }
    }
    None
}

/// Un año solo: «VIGENCIA 2033».
static ANIO: LazyLock<Regex> = LazyLock::new(|| re(r"^(\d{4})\b"));

/// Lo que dice que un número de cuatro cifras es una cantidad y no un año:
/// «cobertura hasta 2000 euros».
const UNIDADES: &[&str] = &[
    "eur", "euro", "euros", "peso", "pesos", "mxn", "usd", "dolares", "dls", "km", "kg", "anos",
    "meses", "dias", "horas", "puntos", "unidades",
];

/// ¿Lo que sigue a cuatro cifras deja que sean un año? No si van pegadas a
/// otro número («2027/05», «2027.5»: eso es otra cosa) —salvo otro año con
/// guion, «2023-2033»— ni si les sigue una unidad o una moneda («2000 euros»,
/// «2000 €»).
fn es_un_anio_solo(resto: &str) -> bool {
    let mut cs = resto.chars();
    if let Some(sep @ ('/' | '.' | '-' | ',')) = cs.next() {
        let cifras = cs.take_while(char::is_ascii_digit).count();
        if cifras > 0 && !(sep == '-' && cifras == 4) {
            return false;
        }
    }
    let siguiente = resto.trim_start();
    if siguiente.starts_with(['€', '$', '%']) {
        return false;
    }
    let palabra = siguiente
        .split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or("");
    !UNIDADES.contains(&palabra)
}

/// Lo que une las dos fechas de un intervalo: «del 1/9/2026 **al** 31/8/2027».
static HACIA: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\s*(?:-|–|—|\bal\b|\ba\b|\bhasta\b)\s*(?:el\s+|la\s+)?"));

fn separador(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            ':' | '.' | ',' | ';' | '/' | '(' | ')' | '-' | '–' | '—' | '*' | '|' | '"' | '«' | '»'
        )
}

/// Tras una palabra de caducidad: salta el hueco y lee la fecha. Si lo que
/// viene es un intervalo, devuelve su final. Devuelve la fecha y dónde acaba,
/// relativo a `s`.
///
/// `anio_suelto` deja leer un año solo; no tras un «hasta» suelto, que es la
/// palabra más débil y la que más cantidades lleva detrás («cobertura hasta
/// 2000…»).
fn tras_la_palabra(s: &str, anio_suelto: bool) -> Option<(Fecha, usize)> {
    let mut i = 0;
    let mut libres = 0;
    let mut desde = false;
    loop {
        i += s[i..].find(|c: char| !separador(c)).unwrap_or(s.len() - i);
        if i > HUECO || i >= s.len() {
            return None;
        }
        if let Some((fecha, largo)) = fecha_al_inicio(&s[i..], anio_suelto) {
            let fin = i + largo;
            // ¿Es el principio de un intervalo? Entonces vale el final.
            if let Some(h) = HACIA.find(&s[fin..]) {
                let otra = fin + h.end();
                if let Some((hasta, largo2)) = fecha_al_inicio(&s[otra..], anio_suelto) {
                    if hasta > fecha {
                        return Some((hasta, otra + largo2));
                    }
                }
            }
            // «vigencia desde el 1/1/2026» sin final no es un vencimiento.
            return (!desde).then_some((fecha, fin));
        }
        let largo = s[i..]
            .find(|c: char| !c.is_alphanumeric())
            .unwrap_or(s.len() - i);
        if largo == 0 {
            return None; // un signo que no es separador: aquí no hay fecha
        }
        let palabra = &s[i..i + largo];
        if palabra.chars().any(|c| c.is_ascii_digit()) || FRENO.contains(&palabra) {
            return None;
        }
        if palabra == "desde" {
            desde = true;
        } else if !RELLENO.contains(&palabra) {
            libres += 1;
            if libres > LIBRES {
                return None;
            }
        }
        i += largo;
    }
}

/// ¿Hay contexto de vigencia poco antes de un «hasta» suelto?
fn con_contexto(plegado: &str, inicio: usize) -> bool {
    let mut desde = inicio.saturating_sub(VENTANA_HASTA);
    while !plegado.is_char_boundary(desde) {
        desde += 1;
    }
    let antes = &plegado[desde..inicio];
    CONTEXTO_HASTA.iter().any(|w| antes.contains(w))
}

/// Todas las fechas de caducidad candidatas de un texto, en orden de
/// aparición. Puede haber la misma fecha más de una vez; elegir es cosa de
/// quien llama.
pub fn scan(texto: &str) -> Vec<Candidato> {
    let p = plegar(texto);
    let mut out = Vec::new();
    for c in PALABRA.captures_iter(&p.texto) {
        let m = c.get(0).expect("toda captura tiene grupo 0");
        let regla = if c.name("caducidad").is_some() {
            Regla::Caducidad
        } else if c.name("validez").is_some() {
            Regla::Validez
        } else if c.name("vencimiento").is_some() {
            Regla::Vencimiento
        } else if c.name("vigencia").is_some() {
            Regla::Vigencia
        } else {
            Regla::Hasta
        };
        if regla == Regla::Hasta && !con_contexto(&p.texto, m.start()) {
            continue;
        }
        let Some((fecha, fin)) = tras_la_palabra(&p.texto[m.end()..], regla != Regla::Hasta) else {
            continue;
        };
        out.push(Candidato {
            fecha,
            regla,
            inicio: p.origen[m.start()],
            fin: p.origen[m.end() + fin],
            anios: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn una(texto: &str) -> Option<(String, &'static str)> {
        let c = scan(texto);
        c.first().map(|c| (c.fecha.iso(), c.regla.as_str()))
    }

    fn f(iso: &str, regla: &'static str) -> Option<(String, &'static str)> {
        Some((iso.to_string(), regla))
    }

    #[test]
    fn los_ejemplos_de_la_boveda_de_prueba() {
        assert_eq!(
            una(
                "Periodo de cobertura: desde el 1 de septiembre de 2026 hasta el 31 de agosto de 2027."
            ),
            f("2027-08-31", "hasta")
        );
        assert_eq!(
            una("Tarjeta sanitaria individual, válida hasta el 15/11/2026."),
            f("2026-11-15", "validez")
        );
        assert_eq!(
            una("El contrato vence el 30 de septiembre de 2026 y se renueva por un año."),
            f("2026-09-30", "vencimiento")
        );
        assert_eq!(
            una("TARJETA DE IDENTIDAD DE EXTRANJERO\nNIE X1234567L\nVALIDEZ 15 11 2026"),
            f("2026-11-15", "validez")
        );
        assert_eq!(
            una("VISA CLASSIC\nCADUCA 12/2028"),
            f("2028-12-31", "caducidad")
        );
    }

    #[test]
    fn otros_formatos() {
        assert_eq!(
            una("Fecha de caducidad / Date of expiry 12 MAR 2031"),
            f("2031-03-12", "caducidad")
        );
        assert_eq!(
            una("Caducidad: 15 NOV/NOV 2026"),
            f("2026-11-15", "caducidad")
        );
        assert_eq!(una("expira el 2027-01-31"), f("2027-01-31", "caducidad"));
        assert_eq!(una("Validez: 15-11-26"), f("2026-11-15", "validez"));
        assert_eq!(una("VALID THRU 12/28"), f("2028-12-31", "validez"));
        assert_eq!(una("vigencia: agosto de 2027"), f("2027-08-31", "vigencia"));
        assert_eq!(
            una("Validez del permiso de residencia: 15/11/2026"),
            f("2026-11-15", "validez")
        );
        assert_eq!(
            una("vence el día 30 de septiembre de 2026"),
            f("2026-09-30", "vencimiento")
        );
    }

    #[test]
    fn el_anio_solo_de_la_ine_vale_hasta_fin_de_anio() {
        assert_eq!(
            una("CREDENCIAL PARA VOTAR\nEMISIÓN 2023 VIGENCIA 2023 - 2033"),
            f("2033-12-31", "vigencia")
        );
        assert_eq!(una("VIGENCIA 2023-2033"), f("2033-12-31", "vigencia"));
        assert_eq!(una("Vigencia: 2030"), f("2030-12-31", "vigencia"));
        assert_eq!(una("válida hasta 2031."), f("2031-12-31", "validez"));
    }

    #[test]
    fn cuatro_cifras_que_no_son_un_anio() {
        // Una cantidad.
        assert!(scan("Validez: 2000 euros de cobertura").is_empty());
        assert!(scan("vigencia 2000 €").is_empty());
        // Tras un «hasta» suelto nunca, aunque haya contexto.
        assert!(scan("Cobertura de la póliza hasta 2000").is_empty());
        // Pegado a otro número es otra cosa.
        assert!(scan("caducidad 2027/05").is_empty());
        assert!(scan("vigencia 2027.5").is_empty());
        // Fuera de 1990–2100 no es un año de documento.
        assert!(scan("vigencia 1234").is_empty());
    }

    #[test]
    fn un_intervalo_vale_por_su_final() {
        assert_eq!(
            una("Vigencia: del 1 de septiembre de 2026 al 31 de agosto de 2027"),
            f("2027-08-31", "vigencia")
        );
        assert_eq!(
            una("Vigencia desde el 01/01/2026 hasta el 31/12/2026"),
            f("2026-12-31", "vigencia")
        );
    }

    #[test]
    fn una_fecha_sin_palabra_de_caducidad_no_cuenta() {
        assert!(scan("Fecha de nacimiento 03 04 2004. Madrid, 12 de marzo de 2021.").is_empty());
        assert!(scan("Fecha de expedición: 15/11/2021").is_empty());
    }

    #[test]
    fn un_freno_en_medio_la_descarta() {
        assert!(scan("Validez de la fecha de expedición 15/11/2021").is_empty());
        assert!(scan("vigencia desde el 01/01/2026").is_empty());
    }

    #[test]
    fn hasta_suelto_sin_contexto_no_cuenta() {
        assert!(scan("Horario de atención hasta el 30/09/2026 por obras.").is_empty());
        assert!(scan("Abierto hasta las 14:00").is_empty());
    }

    #[test]
    fn una_fecha_imposible_no_se_lee() {
        assert!(scan("Validez 31/02/2027").is_empty());
        // Día/mes, nunca al estilo de Estados Unidos: 11/31 no existe.
        assert!(scan("Validez 11/31/2027").is_empty());
    }

    #[test]
    fn la_evidencia_apunta_al_sitio_exacto_del_original() {
        let t = "Titular: José Núñez. VÁLIDA HASTA el 15/11/2026, sin prórroga.";
        let c = scan(t)[0];
        assert_eq!(&t[c.inicio..c.fin], "VÁLIDA HASTA el 15/11/2026");
    }

    #[test]
    fn varias_fechas_salen_todas_en_orden() {
        // La misma fecha puede salir por dos palabras: «vigencia» y el «hasta»
        // que la sigue. Deduplicar es cosa de quien elige.
        let t = "Póliza de salud. Carencia hasta el 31/12/2026 para el dental. \
                 Vigencia de la póliza hasta el 31/08/2027.";
        let c: Vec<_> = scan(t)
            .iter()
            .map(|c| (c.fecha.iso(), c.regla.as_str()))
            .collect();
        assert_eq!(
            c,
            [
                ("2026-12-31".to_string(), "hasta"),
                ("2027-08-31".to_string(), "vigencia"),
                ("2027-08-31".to_string(), "hasta"),
            ]
        );
    }
}
