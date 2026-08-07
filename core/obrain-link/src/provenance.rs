//! `downloaded_from` — la URL de origen de cada archivo descargado.
//!
//! Windows guarda en un **flujo alternativo NTFS** llamado `:Zone.Identifier`
//! la procedencia de todo lo que se descarga. Es un dato exacto, gratuito, que
//! lleva ahí veinte años, y que prácticamente ningún indexador usa.
//!
//! Conecta un PDF con la web de la que salió, y de ahí —cuando lleguen los
//! conectores— con el correo o el trámite que lo originó. Para una factura de
//! un banco o un formulario de un consulado, esa arista es exactamente la que
//! reconstruye la historia.
//!
//! El contenido del flujo es un INI:
//!
//! ```text
//! [ZoneTransfer]
//! ZoneId=3
//! ReferrerUrl=https://banco.example/area-cliente
//! HostUrl=https://banco.example/descargas/nomina_marzo.pdf
//! ```

use crate::{write_edge, Edge, FileView, LinkReport};
use obrain_core::{Method, Result, Store, Value};

/// Extrae la mejor URL disponible. `HostUrl` es la del archivo en sí;
/// `ReferrerUrl` es la página desde la que se pinchó. Se prefiere la primera.
pub fn parse_zone_identifier(contents: &str) -> Option<String> {
    let mut referrer = None;
    for line in contents.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("HostUrl=") {
            let v = v.trim();
            if !v.is_empty() && v != "about:internet" {
                return Some(v.to_string());
            }
        } else if let Some(v) = line.strip_prefix("ReferrerUrl=") {
            let v = v.trim();
            if !v.is_empty() {
                referrer = Some(v.to_string());
            }
        }
    }
    referrer
}

/// Lee el flujo alternativo. En Windows basta con abrir `ruta:flujo`.
#[cfg(windows)]
fn read_zone(path: &str) -> Option<String> {
    let stream = format!("{path}:Zone.Identifier");
    // Los flujos son diminutos; si algo devuelve más de 8 KB, no es esto.
    let bytes = std::fs::read(&stream).ok()?;
    if bytes.len() > 8192 {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(not(windows))]
fn read_zone(_path: &str) -> Option<String> {
    None // los flujos alternativos son de NTFS
}

pub fn link(store: &Store, files: &[FileView], at: i64, report: &mut LinkReport) -> Result<()> {
    for f in files {
        // Un archivo deshidratado de nube podría provocar tráfico al tocar sus
        // flujos. El invariante de M1 manda: si no es local, no se toca.
        if !f.is_local() {
            continue;
        }
        let Some(contents) = read_zone(&f.path) else {
            continue;
        };
        let Some(url) = parse_zone_identifier(&contents) else {
            continue;
        };
        if write_edge(
            store,
            report,
            Edge {
                subject: &f.entity,
                predicate: "downloaded_from",
                object: Value::Text(url),
                method: Method::Deterministic,
                confidence: 1.0,
                source: "linker:zone-identifier",
            },
            at,
        )? {
            report.downloaded_from += 1;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_zone_identifier;

    #[test]
    fn prefiere_hosturl_sobre_referrer() {
        let ini = "[ZoneTransfer]\r\nZoneId=3\r\n\
                   ReferrerUrl=https://banco.example/area\r\n\
                   HostUrl=https://banco.example/nomina.pdf\r\n";
        assert_eq!(
            parse_zone_identifier(ini).as_deref(),
            Some("https://banco.example/nomina.pdf")
        );
    }

    #[test]
    fn cae_a_referrer_si_no_hay_hosturl() {
        let ini = "[ZoneTransfer]\r\nZoneId=3\r\nReferrerUrl=https://consulado.example/visados\r\n";
        assert_eq!(
            parse_zone_identifier(ini).as_deref(),
            Some("https://consulado.example/visados")
        );
    }

    #[test]
    fn ignora_el_marcador_generico_de_internet() {
        // Muchos archivos llevan solo esto: no dice de donde vienen.
        let ini = "[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=about:internet\r\n";
        assert_eq!(parse_zone_identifier(ini), None);
    }

    #[test]
    fn sin_zona_no_hay_arista() {
        assert_eq!(parse_zone_identifier("[ZoneTransfer]\r\nZoneId=3\r\n"), None);
        assert_eq!(parse_zone_identifier(""), None);
    }
}
