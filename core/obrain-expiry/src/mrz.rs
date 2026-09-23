//! La **zona de lectura mecánica** (MRZ): las líneas de `<<<` al pie de un
//! pasaporte, o al dorso de la TIE, del DNI o de la INE.
//!
//! Es la fecha de caducidad mejor escrita del documento: en un sitio fijo, en
//! una letra hecha para leerse a máquina (OCR-B) y **con dígito de control**
//! (ICAO 9303). Si el OCR lee un 8 donde hay un 3, el dígito no cuadra y la
//! fecha se descarta en vez de salir otra. Por eso pesa más que cualquier
//! palabra ([ADR-0028](../../../docs/adr/0028-vencimientos-sin-palabra.md)).
//!
//! Se leen las dos formas que llevan los documentos personales:
//!
//! ```text
//! TD3 (pasaporte, 2 × 44)     L898902C36UTO7408122F1204159ZE184226B<<<<<10
//!                             └ número ┘└──┘└nac.─┘│└cad.─┘
//! TD1 (tarjeta, 3 × 30)       7408122F1204159UTO<<<<<<<<<<<6
//!                             └nac.─┘│└cad.─┘└──┘
//! ```
//!
//! (El TD2 de algunos visados tiene la misma segunda línea que el TD3, así que
//! también se lee.) Sólo cuentan si cuadran **los dos** dígitos de control, el
//! del nacimiento y el de la caducidad: una línea cualquiera de texto no pasa
//! por casualidad.

use crate::fecha::Fecha;
use crate::texto::{Candidato, Regla};

/// Un carácter de la MRZ tal y como lo dejó el OCR, con su sitio en el texto.
#[derive(Debug, Clone, Copy)]
struct Letra {
    c: char,
    byte: usize,
    largo: usize,
}

/// El carácter, como se escribe en una MRZ. El OCR lee a veces el `<` como
/// `«` o `‹`, y mete espacios donde no los hay: los espacios se saltan antes.
fn normalizar(c: char) -> char {
    match c {
        '<' | '«' | '‹' | '〈' | '＜' => '<',
        c if c.is_ascii_alphanumeric() => c.to_ascii_uppercase(),
        _ => '?',
    }
}

/// Una cifra de un campo numérico. Ahí no puede haber letras, así que las que
/// el OCR confunde con cifras se leen como la cifra: el dígito de control
/// dirá si era eso.
fn cifra(c: char) -> Option<u32> {
    match c {
        '0'..='9' => c.to_digit(10),
        'O' | 'Q' | 'D' => Some(0),
        'I' | 'L' => Some(1),
        'Z' => Some(2),
        'S' => Some(5),
        'B' => Some(8),
        _ => None,
    }
}

/// El dígito de control de ICAO 9303: pesos 7, 3, 1 y módulo 10.
fn control(cifras: &[u32]) -> u32 {
    cifras
        .iter()
        .zip([7, 3, 1].iter().cycle())
        .map(|(c, p)| c * p)
        .sum::<u32>()
        % 10
}

/// Seis cifras y su dígito de control, si cuadran.
fn campo(letras: &[Letra]) -> Option<[u32; 6]> {
    let mut v = [0; 6];
    for (i, l) in letras[..6].iter().enumerate() {
        v[i] = cifra(l.c)?;
    }
    (control(&v) == cifra(letras[6].c)?).then_some(v)
}

/// `AAMMDD` → fecha. Una caducidad del 70 en adelante es del siglo pasado.
fn caducidad(v: [u32; 6]) -> Option<Fecha> {
    let aa = (v[0] * 10 + v[1]) as i32;
    let y = if aa >= 70 { 1900 + aa } else { 2000 + aa };
    Fecha::new(y, v[2] * 10 + v[3], v[4] * 10 + v[5])
}

/// Un nacimiento `AAMMDD` que puede existir: mes y día con sentido.
fn nacimiento(v: [u32; 6]) -> bool {
    let (m, d) = (v[2] * 10 + v[3], v[4] * 10 + v[5]);
    (1..=12).contains(&m) && (1..=31).contains(&d)
}

fn sexo(c: char) -> bool {
    matches!(c, 'M' | 'F' | 'X' | '<')
}

fn nacionalidad(l: &[Letra]) -> bool {
    l.iter().all(|l| l.c.is_ascii_uppercase() || l.c == '<')
}

/// La caducidad de una segunda línea que empieza en `k`, y dónde está: con el
/// patrón del TD3 (y TD2) o con el del TD1.
fn en(letras: &[Letra], k: usize) -> Option<(Fecha, usize, usize)> {
    let r = &letras[k..];
    // TD3 / TD2: número(9) control nacionalidad(3) nacimiento(6) control sexo
    // caducidad(6) control…
    if r.len() >= 28
        && nacionalidad(&r[10..13])
        && sexo(r[20].c)
        && campo(&r[13..20]).is_some_and(nacimiento)
    {
        if let Some(f) = campo(&r[21..28]).and_then(caducidad) {
            return Some((f, 21, 28));
        }
    }
    // TD1: nacimiento(6) control sexo caducidad(6) control nacionalidad(3)…
    if r.len() >= 18
        && sexo(r[7].c)
        && nacionalidad(&r[15..18])
        && campo(&r[0..7]).is_some_and(nacimiento)
    {
        if let Some(f) = campo(&r[8..15]).and_then(caducidad) {
            return Some((f, 8, 15));
        }
    }
    None
}

/// Las caducidades de las MRZ de un texto, con el tramo del campo (fecha y su
/// dígito de control) en bytes del original.
pub fn scan(texto: &str) -> Vec<Candidato> {
    let mut out: Vec<Candidato> = Vec::new();
    let mut base = 0;
    for linea in texto.split_inclusive('\n') {
        let letras: Vec<Letra> = linea
            .char_indices()
            .filter(|(_, c)| !c.is_whitespace())
            .map(|(i, c)| Letra {
                c: normalizar(c),
                byte: base + i,
                largo: c.len_utf8(),
            })
            .collect();
        base += linea.len();
        // No se exige ningún `<`: el OCR a veces los lee como `K` o `C`, y lo
        // que protege de un texto cualquiera son los dos dígitos de control.
        for k in 0..letras.len() {
            let Some((fecha, a, b)) = en(&letras, k) else {
                continue;
            };
            let (inicio, ultima) = (letras[k + a].byte, letras[k + b - 1]);
            let fin = ultima.byte + ultima.largo;
            // El TD3 también encaja como TD1 trece letras más allá: la misma
            // fecha en el mismo sitio se cuenta una vez.
            if !out.iter().any(|c| c.fecha == fecha && c.inicio == inicio) {
                out.push(Candidato {
                    fecha,
                    regla: Regla::Mrz,
                    inicio,
                    fin,
                    anios: None,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fechas(texto: &str) -> Vec<String> {
        scan(texto).iter().map(|c| c.fecha.iso()).collect()
    }

    /// El espécimen de ICAO 9303: Anna Maria Eriksson, de Utopía.
    const TD3: &str = "P<UTOERIKSSON<<ANNA<MARIA<<<<<<<<<<<<<<<<<<<\nL898902C36UTO7408122F1204159ZE184226B<<<<<10\n";
    const TD1: &str = "I<UTOD231458907<<<<<<<<<<<<<<<\n7408122F1204159UTO<<<<<<<<<<<6\nERIKSSON<<ANNA<MARIA<<<<<<<<<<\n";

    #[test]
    fn el_especimen_de_icao_en_pasaporte_y_en_tarjeta() {
        assert_eq!(fechas(TD3), ["2012-04-15"]);
        assert_eq!(fechas(TD1), ["2012-04-15"]);
        // El tramo es el campo de la caducidad con su dígito de control.
        let c = scan(TD3)[0];
        assert_eq!(&TD3[c.inicio..c.fin], "1204159");
        assert_eq!(c.regla, Regla::Mrz);
    }

    #[test]
    fn un_pasaporte_mexicano_con_el_ruido_del_ocr() {
        // Espacios metidos, `«` por `<` y una O por un cero en la fecha.
        let ocr = "P«MEXPRUEBA«GARCIA««JUAN««««««««««««««««««««\nG00000000 2 MEX 040104 3 M 32O6102 <<<<<<<<<<<<<<04\n";
        assert_eq!(fechas(ocr), ["2032-06-10"]);
    }

    #[test]
    fn un_digito_de_control_que_no_cuadra_no_da_fecha() {
        // El 9 de control leído como 8: podría ser otra fecha, así que ninguna.
        assert!(scan("L898902C36UTO7408122F1204158ZE184226B<<<<<10").is_empty());
        // Y el nacimiento también tiene que cuadrar.
        assert!(scan("L898902C36UTO7408127F1204159ZE184226B<<<<<10").is_empty());
    }

    #[test]
    fn un_texto_cualquiera_no_es_una_mrz() {
        // Tiene la forma (seis cifras, una, M, seis cifras, una, MEX) pero el
        // control del nacimiento no cuadra.
        assert!(scan("Folio 0512238M2611305MEX <ver anexo>").is_empty());
        assert!(scan("Nómina 2026-08 · 1.234,56 € · IBAN ES00 0000 0000").is_empty());
    }

    #[test]
    fn sin_signos_de_menor_tambien_se_lee() {
        // El OCR puede leer los `<` como `K`: los controles bastan.
        assert_eq!(
            fechas("L898902C36UTO7408122F1204159ZE184226BKKKKK10"),
            ["2012-04-15"]
        );
    }
}
