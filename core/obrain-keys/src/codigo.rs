//! La clave de recuperación: 25 caracteres en base32 de Crockford.
//!
//! Es lo que una persona apunta en un papel o guarda en su gestor de
//! contraseñas, así que se diseña para eso: sin letras que se confundan (no hay
//! `I`, `L`, `O` ni `U`), en cinco grupos de cinco, y perdonando al leerla lo
//! que una persona hace al teclearla —minúsculas, guiones, espacios, una `O`
//! por un `0`—. 25 símbolos de 32 son 125 bits de azar: no hace falta una
//! derivación lenta para que no se pueda adivinar, y aun así se deriva con
//! Argon2id.

use crate::{Error, Result, azar};
use zeroize::Zeroizing;

const ALFABETO: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const LARGO: usize = 25;
const GRUPO: usize = 5;

/// Una clave de recuperación. Su `Debug` no la enseña.
pub struct RecoveryCode(Zeroizing<String>);

impl RecoveryCode {
    pub fn generate() -> Result<Self> {
        let mut bytes = Zeroizing::new([0u8; LARGO]);
        azar(bytes.as_mut())?;
        // 256 es múltiplo de 32: quedarse con 5 bits de cada byte no sesga.
        let s: String = bytes
            .iter()
            .map(|b| ALFABETO[(b & 31) as usize] as char)
            .collect();
        Ok(RecoveryCode(Zeroizing::new(s)))
    }

    /// Lee lo que escribió una persona.
    pub fn parse(texto: &str) -> Result<Self> {
        let mut s = Zeroizing::new(String::with_capacity(LARGO));
        for c in texto.chars() {
            if c == '-' || c.is_whitespace() {
                continue;
            }
            let c = match c.to_ascii_uppercase() {
                'O' => '0',
                'I' | 'L' => '1',
                c => c,
            };
            if !c.is_ascii() || !ALFABETO.contains(&(c as u8)) {
                return Err(Error::CodigoMalFormado);
            }
            s.push(c);
        }
        if s.len() != LARGO {
            return Err(Error::CodigoMalFormado);
        }
        Ok(RecoveryCode(s))
    }

    /// Sin guiones: lo que entra en la derivación.
    pub fn normalized(&self) -> &str {
        &self.0
    }
}

/// En grupos: `K7Q2M-9XP4T-R8B3N-…`. Es como se enseña y como se apunta.
impl std::fmt::Display for RecoveryCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, trozo) in self.0.as_bytes().chunks(GRUPO).enumerate() {
            if i > 0 {
                f.write_str("-")?;
            }
            f.write_str(std::str::from_utf8(trozo).map_err(|_| std::fmt::Error)?)?;
        }
        Ok(())
    }
}

impl std::fmt::Debug for RecoveryCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RecoveryCode(…)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn se_genera_con_la_forma_de_siempre() {
        let c = RecoveryCode::generate().unwrap();
        let s = c.to_string();
        assert_eq!(s.len(), 29, "25 símbolos y 4 guiones: {s}");
        assert_eq!(s.split('-').count(), 5);
        assert!(c.normalized().bytes().all(|b| ALFABETO.contains(&b)));
        assert_ne!(
            c.normalized(),
            RecoveryCode::generate().unwrap().normalized()
        );
    }

    /// Lo que una persona hace al teclearla no la invalida.
    #[test]
    fn se_perdona_lo_que_hace_una_persona_al_teclearla() {
        let c = RecoveryCode::generate().unwrap();
        let tecleada = c.to_string().to_lowercase().replace('-', " - ");
        assert_eq!(
            RecoveryCode::parse(&tecleada).unwrap().normalized(),
            c.normalized()
        );

        let con_letras = "O0000-I1111-L2222-33333-44444";
        assert_eq!(
            RecoveryCode::parse(con_letras).unwrap().normalized(),
            "0000011111122223333344444"
        );
    }

    #[test]
    fn lo_que_no_es_una_clave_se_rechaza() {
        assert!(RecoveryCode::parse("").is_err());
        assert!(RecoveryCode::parse("ABCDE-ABCDE").is_err(), "corta");
        assert!(
            RecoveryCode::parse("UUUUU-AAAAA-BBBBB-CCCCC-DDDDD").is_err(),
            "la U no está"
        );
        assert!(RecoveryCode::parse("ÁÁÁÁÁ-AAAAA-BBBBB-CCCCC-DDDDD").is_err());
    }

    #[test]
    fn el_debug_no_la_enseña() {
        assert_eq!(
            format!("{:?}", RecoveryCode::generate().unwrap()),
            "RecoveryCode(…)"
        );
    }
}
