//! Una fecha de calendario: día, mes y año, sin hora ni zona.
//!
//! Un vencimiento no es un instante. «La TIE caduca el 15/11/2026» es verdad
//! en Málaga y en Guadalajara, y guardarlo como milisegundos obligaría a
//! elegir una zona horaria que lo haría cambiar de día según quién lo mire.
//! Por eso en el log va como texto ISO (`2026-11-15`) y aquí como tres números.

use std::fmt;

/// Años que se aceptan como fecha de un documento. Fuera de aquí, lo que
/// parece una fecha es otra cosa (un número de expediente, un OCR torcido).
const PRIMER_ANIO: i32 = 1990;
const ULTIMO_ANIO: i32 = 2100;

/// Día, mes y año. El orden de los campos es el del calendario, así que el
/// `Ord` derivado ordena de la más antigua a la más reciente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fecha {
    pub y: i32,
    pub m: u32,
    pub d: u32,
}

impl Fecha {
    /// Una fecha que existe: nada de 31 de febrero ni de años imposibles.
    pub fn new(y: i32, m: u32, d: u32) -> Option<Self> {
        let valida = (PRIMER_ANIO..=ULTIMO_ANIO).contains(&y)
            && (1..=12).contains(&m)
            && d >= 1
            && d <= dias_del_mes(y, m);
        valida.then_some(Fecha { y, m, d })
    }

    /// El último día del mes. Es lo que significa «CADUCA 12/2028» en una
    /// tarjeta: vale durante todo diciembre.
    pub fn fin_de_mes(y: i32, m: u32) -> Option<Self> {
        if !(1..=12).contains(&m) {
            return None;
        }
        Fecha::new(y, m, dias_del_mes(y, m))
    }

    /// `2026-11-15`. Es la forma en que vive en el log.
    pub fn iso(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.y, self.m, self.d)
    }

    /// La contraria de [`Fecha::iso`]. Estricta: sólo `AAAA-MM-DD`.
    pub fn parse_iso(s: &str) -> Option<Self> {
        let b = s.as_bytes();
        if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
            return None;
        }
        Fecha::new(
            s[0..4].parse().ok()?,
            s[5..7].parse().ok()?,
            s[8..10].parse().ok()?,
        )
    }

    /// Días desde el 1 de enero de 1970. Es la escala en la que se resta:
    /// «en 53 días» es `vence.dias() - hoy.dias()`.
    ///
    /// El algoritmo es el de los días civiles de Howard Hinnant: exacto para
    /// cualquier año del calendario gregoriano y sin tablas.
    pub fn dias(&self) -> i64 {
        let y = i64::from(if self.m <= 2 { self.y - 1 } else { self.y });
        let m = i64::from(self.m);
        let d = i64::from(self.d);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// La contraria de [`Fecha::dias`].
    pub fn desde_dias(z: i64) -> Self {
        let z = z + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + i64::from(m <= 2);
        Fecha {
            y: y as i32,
            m: m as u32,
            d: d as u32,
        }
    }

    /// El día de hoy **en UTC** a partir de un instante en milisegundos.
    ///
    /// El núcleo no conoce la zona horaria de quien mira. Entre la medianoche
    /// de Madrid y la de UTC, «hoy» va un día corrido; la ventana, que sí la
    /// conoce, recalcula lo que enseña.
    pub fn hoy(ms: i64) -> Self {
        Fecha::desde_dias(ms.div_euclid(86_400_000))
    }
}

impl fmt::Display for Fecha {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.iso())
    }
}

fn bisiesto(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn dias_del_mes(y: i32, m: u32) -> u32 {
    match m {
        2 if bisiesto(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn una_fecha_imposible_no_es_una_fecha() {
        assert!(Fecha::new(2026, 2, 29).is_none());
        assert!(Fecha::new(2028, 2, 29).is_some());
        assert!(Fecha::new(2026, 13, 1).is_none());
        assert!(Fecha::new(2026, 4, 31).is_none());
        assert!(Fecha::new(1890, 1, 1).is_none());
    }

    #[test]
    fn mes_y_anio_es_el_ultimo_dia_del_mes() {
        assert_eq!(Fecha::fin_de_mes(2028, 12).unwrap().iso(), "2028-12-31");
        assert_eq!(Fecha::fin_de_mes(2028, 2).unwrap().iso(), "2028-02-29");
        assert_eq!(Fecha::fin_de_mes(2027, 2).unwrap().iso(), "2027-02-28");
    }

    #[test]
    fn iso_va_y_vuelve() {
        let f = Fecha::new(2026, 11, 15).unwrap();
        assert_eq!(Fecha::parse_iso(&f.iso()), Some(f));
        assert!(Fecha::parse_iso("15/11/2026").is_none());
        assert!(Fecha::parse_iso("2026-02-30").is_none());
    }

    #[test]
    fn los_dias_cuentan_como_el_calendario() {
        assert_eq!(
            Fecha::desde_dias(0),
            Fecha {
                y: 1970,
                m: 1,
                d: 1
            }
        );
        let hoy = Fecha::new(2026, 9, 23).unwrap();
        let tie = Fecha::new(2026, 11, 15).unwrap();
        assert_eq!(tie.dias() - hoy.dias(), 53);
        for z in [-1, 0, 59, 10_000, 20_718, 47_000] {
            assert_eq!(Fecha::desde_dias(z).dias(), z);
        }
        // 2026-09-23T10:00:00Z
        assert_eq!(Fecha::hoy(1_790_157_600_000), hoy);
    }
}
