//! Una caducidad **sin palabra**: dos fechas a N años justos en un documento
//! de identidad.
//!
//! El pasaporte de JP (2026-09-23) es un PDF escaneado. El OCR de Windows leyó
//! sus tres fechas —nacimiento, expedición y caducidad— pero se comió las
//! etiquetas: de «Fecha de caducidad / Date of expiry» quedó «Fecha de» y
//! «date», a cualquier resolución. Sin palabra, [`crate::texto`] no se fía de
//! ninguna fecha, y es lo correcto: un documento está lleno de fechas.
//!
//! Pero la caducidad de un documento de identidad tiene una huella que no
//! necesita etiqueta: **es la expedición más un número entero de años** —1, 3,
//! 5, 6 o 10 en un pasaporte—, el mismo día y mes o el día anterior. Dos fechas
//! así —con el año en cuatro cifras—, en algo que dice «pasaporte»,
//! «identidad», «residencia», «visa», «licencia de conducir» o «credencial»,
//! son la expedición y la caducidad; la segunda es la que cuenta.
//!
//! Sólo en esos documentos: en una nómina, la fecha de antigüedad y la del mes
//! caen a menudo a años justos (el día 1), y lo que caduca se queda en la
//! tarjeta «Caduca pronto» hasta que se corrige. Y es la regla que menos pesa:
//! si alguna palabra dijo algo, gana la palabra. El porqué, en
//! [ADR-0028](../../../docs/adr/0028-vencimientos-sin-palabra.md).

use crate::fecha::Fecha;
use crate::texto::{Candidato, Regla, fecha_completa_al_inicio, plegar};
use regex::Regex;
use std::sync::LazyLock;

/// Los años de validez que se reconocen. Un pasaporte mexicano vale 1, 3, 6 o
/// 10; uno español, 2, 5 o 10; una TIE, 1, 2 o 5.
const ANIOS: std::ops::RangeInclusive<i32> = 1..=10;

/// Lo que dice que un texto es un documento de identidad, ya plegado.
///
/// «Licencia» sólo de conducir: una licencia de software («license: MIT»)
/// vive en archivos llenos de números de versión que parecen fechas.
static IDENTIDAD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)\b(?:
            pasaporte|passport|identidad|identity\s+card|residencia|residence|visa|visado|credencial
          | licencia\s+(?:de|para)\s+(?:conducir|manejar|manejo)|driv\w*\s+licen[cs]e
        )\b",
    )
    .expect("la expresión de los documentos de identidad es válida")
});

/// ¿La fecha lleva el año con sus cuatro cifras? Un documento de identidad
/// siempre lo escribe así, y un número de versión como `1.10.20` no.
fn anio_de_cuatro(fecha: &str) -> bool {
    let cuatro = |s: &str| s.len() == 4 && s.bytes().all(|b| b.is_ascii_digit());
    fecha.get(..4).is_some_and(cuatro)
        || fecha
            .get(fecha.len().saturating_sub(4)..)
            .is_some_and(cuatro)
}

/// Una fecha con día, dónde empieza y dónde acaba, en bytes del plegado.
#[derive(Debug, Clone, Copy)]
struct Vista {
    fecha: Fecha,
    inicio: usize,
    fin: usize,
}

/// Todas las fechas con día de un texto plegado, en orden. Sólo empiezan
/// donde empieza una palabra: los dígitos de dentro de la CURP o de un
/// número de pasaporte no son una fecha.
fn fechas(plegado: &str) -> Vec<Vista> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut anterior: Option<char> = None;
    while i < plegado.len() {
        let c = plegado[i..].chars().next().expect("i está en un carácter");
        let empieza = c.is_ascii_digit() && !anterior.is_some_and(char::is_alphanumeric);
        if empieza {
            if let Some((fecha, largo)) = fecha_completa_al_inicio(&plegado[i..]) {
                if anio_de_cuatro(&plegado[i..i + largo]) {
                    out.push(Vista {
                        fecha,
                        inicio: i,
                        fin: i + largo,
                    });
                }
                anterior = plegado[..i + largo].chars().next_back();
                i += largo;
                continue;
            }
        }
        anterior = Some(c);
        i += c.len_utf8();
    }
    out
}

/// Cuántos años justos van de `a` a `b`: el mismo día y mes, o el día antes
/// (hay países que caducan la víspera). `None` si no es un plazo entero.
fn anios_justos(a: Fecha, b: Fecha) -> Option<u8> {
    let n = b.y - a.y;
    if !ANIOS.contains(&n) {
        return None;
    }
    // El 29 de febrero, en un año que no lo tiene, cumple el 28.
    let aniversario = Fecha::new(b.y, a.m, a.d).or_else(|| Fecha::new(b.y, 2, 28))?;
    let d = aniversario.dias() - b.dias();
    (d == 0 || d == 1).then_some(n as u8)
}

/// Las caducidades por plazo de un texto: la última fecha de cada pareja a N
/// años justos. Vacío si el texto no es de un documento de identidad.
pub fn scan(texto: &str) -> Vec<Candidato> {
    let p = plegar(texto);
    if !IDENTIDAD.is_match(&p.texto) {
        return Vec::new();
    }
    let vistas = fechas(&p.texto);
    let mut out = Vec::new();
    for (i, x) in vistas.iter().enumerate() {
        for y in &vistas[i + 1..] {
            // La expedición suele ir antes, pero no siempre: se miden en el
            // orden del calendario.
            let (antes, despues) = if x.fecha <= y.fecha { (x, y) } else { (y, x) };
            let Some(n) = anios_justos(antes.fecha, despues.fecha) else {
                continue;
            };
            // El tramo va de una fecha a la otra: el «¿por qué?» marca las dos.
            let (a, b) = (x.inicio.min(y.inicio), x.fin.max(y.fin));
            out.push(Candidato {
                fecha: despues.fecha,
                regla: Regla::Plazo,
                inicio: p.origen[a],
                fin: p.origen[b],
                anios: Some(n),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La forma exacta en que el OCR de Windows leyó el pasaporte de JP, con
    /// los datos inventados: sin «caducidad», sin «expiry» y sin MRZ.
    const PASAPORTE_OCR: &str = "\
ESTADOS UNIDOS MEXICANOS
PASAPORTE
PASSPORT
Clave del pais de expedición
MEX
Passport No
G00000000
Nombres/ Given names
JUAN PRUEBA
Nacionalidad
MEXICANA
CURP Personal No
Fecha de Date ot bith
04 01 2004
PRUE040104HJCRRN09
Sex
Lugar de nacimiento Place of birth
JALISCO
a de expediciónDaXeXiX
10 06 2022
Autoridad Authority SRE-JALISCO
123456
Fecha de
date
10 06 2032
";

    fn plazos(texto: &str) -> Vec<(String, u8)> {
        scan(texto)
            .iter()
            .map(|c| (c.fecha.iso(), c.anios.unwrap()))
            .collect()
    }

    #[test]
    fn el_pasaporte_sin_etiquetas_caduca_a_los_diez_anios_de_expedirse() {
        assert_eq!(plazos(PASAPORTE_OCR), [("2032-06-10".to_string(), 10)]);
        // Y no ve fechas dentro de la CURP (…040104…).
        let c = scan(PASAPORTE_OCR)[0];
        let tramo = &PASAPORTE_OCR[c.inicio..c.fin];
        assert!(tramo.starts_with("10 06 2022") && tramo.ends_with("10 06 2032"));
        assert_eq!(c.regla, Regla::Plazo);
    }

    #[test]
    fn la_vispera_tambien_cuenta_y_el_orden_no_importa() {
        assert_eq!(
            plazos("PASSPORT\nDate of expiry 14/03/2033\nDate of issue 15/03/2023"),
            [("2033-03-14".to_string(), 10)]
        );
        assert_eq!(
            plazos("Tarjeta de residencia 29/02/2024 · 28/02/2029"),
            [("2029-02-28".to_string(), 5)]
        );
    }

    #[test]
    fn fuera_de_un_documento_de_identidad_no_cuenta() {
        // Una nómina: la antigüedad y el mes a diez años justos.
        assert!(
            scan("Nómina de agosto. Fecha de alta 01/08/2016. Periodo 01/08/2026 a 31/08/2026")
                .is_empty()
        );
        // Una licencia de software no es una licencia de conducir.
        assert!(scan("Licencia MIT, 01/08/2016 — 01/08/2026").is_empty());
        assert!(!scan("LICENCIA DE CONDUCIR · 01/08/2021 · 01/08/2026").is_empty());
    }

    #[test]
    fn un_numero_de_version_no_es_una_fecha() {
        // Así salió la falsa alarma en un `package-lock.json`: `1.10.20` y
        // `1.10.30` son el 1 de octubre de 2020 y de 2030 para el lector de
        // fechas, a diez años justos. Y `passport` también es un paquete.
        let lock =
            r#""passport": "^0.6.0", "a": { "version": "1.10.20" }, "b": { "version": "1.10.30" }"#;
        assert!(scan(lock).is_empty());
    }

    #[test]
    fn fechas_sin_plazo_entero_no_cuentan() {
        // Nacimiento y expedición: 18 años y 5 meses.
        assert!(scan("PASAPORTE\n04 01 2004\n10 06 2022").is_empty());
        // Más de diez años no es una validez.
        assert!(scan("PASAPORTE\n10 06 2010\n10 06 2022").is_empty());
        // Ni un mes y medio.
        assert!(scan("PASAPORTE\n10 06 2022\n25 07 2022").is_empty());
    }
}
