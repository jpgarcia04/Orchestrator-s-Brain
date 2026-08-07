//! Texto plano, configuración y código. El extractor más simple y el que más
//! archivos toca en un disco real.

use crate::text::{decode, Decoded};

pub fn extract(bytes: &[u8]) -> Result<String, String> {
    match decode(bytes) {
        Decoded::Text(t) => Ok(t),
        // No es un fallo: es un archivo que se llamaba `.log` y era una base
        // de datos LevelDB. Lo dice el informe, no una excepción.
        Decoded::Binary => Err("binario".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_markdown_sale_tal_cual() {
        assert_eq!(extract(b"# Titulo\n\ntexto").unwrap(), "# Titulo\n\ntexto");
    }

    #[test]
    fn un_leveldb_disfrazado_de_log_se_rechaza() {
        assert!(extract(b"\x01\x00\x00\x00leveldb.BytewiseComparator").is_err());
    }
}
