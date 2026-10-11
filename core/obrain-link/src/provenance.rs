//! `downloaded_from` — el origen web de cada archivo descargado.
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

use crate::{Edge, FileView, LinkReport, write_edge};
use obrain_core::{Method, Result, Store, Value};

/// Conserva sólo esquema, host y puerto. La ruta también puede ser un enlace
/// de acceso firmado; no intentamos adivinar qué parámetros son secretos.
pub fn safe_origin(value: &str) -> Option<String> {
    let url = url::Url::parse(value).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return None;
    }
    Some(url.origin().ascii_serialization())
}

/// Extrae el origen web de la mejor URL disponible, sin conservar acceso ni
/// rutas. Se prefiere `HostUrl` y se usa `ReferrerUrl` como alternativa.
pub fn parse_zone_identifier(contents: &str) -> Option<String> {
    let mut referrer = None;
    for line in contents.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("HostUrl=") {
            let v = v.trim();
            if !v.is_empty() && v != "about:internet" {
                if let Some(origin) = safe_origin(v) {
                    return Some(origin);
                }
            }
        } else if let Some(v) = line.strip_prefix("ReferrerUrl=") {
            let v = v.trim();
            if !v.is_empty() {
                referrer = safe_origin(v);
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

    #[cfg(windows)]
    #[test]
    fn la_observacion_nueva_guarda_solo_el_origen() {
        use obrain_core::{Store, TimePoint};
        let path =
            std::env::temp_dir().join(format!("obrain-zone-{}.txt", obrain_core::EntityId::new()));
        std::fs::write(&path, "fixture").unwrap();
        std::fs::write(format!("{}:Zone.Identifier", path.display()), "HostUrl=https://user:password@portal.example/acceso/SINTETICO?token=SINTETICO#SINTETICO").unwrap();
        let store = Store::open_in_memory().unwrap();
        let e = store.create_entity(1).unwrap();
        let file = crate::FileView {
            entity: e.clone(),
            path: path.to_string_lossy().into(),
            hash: None,
            modified_ms: 1,
            size: 7,
            availability: "local".into(),
        };
        let mut report = crate::LinkReport::default();
        super::link(&store, &[file], 1, &mut report).unwrap();
        let a = store
            .resolve(&e, "downloaded_from", TimePoint::at(1))
            .unwrap()
            .unwrap();
        assert_eq!(a.object.as_text(), Some("https://portal.example"));
        assert!(a.evidence.is_none());
        assert_eq!(report.downloaded_from, 1);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn no_persiste_acceso_ni_rutas_y_rechaza_esquemas_ajenos() {
        assert_eq!(
            parse_zone_identifier(
                "HostUrl=https://user:password@portal.example:8443/acceso/SINTETICO?token=SINTETICO#SINTETICO"
            ),
            Some("https://portal.example:8443".into())
        );
        for url in [
            "file:///fixtures/documento.txt",
            "javascript:alert(1)",
            "no-url",
        ] {
            assert_eq!(parse_zone_identifier(&format!("HostUrl={url}")), None);
        }
        assert_eq!(
            parse_zone_identifier(
                "HostUrl=about:internet\nReferrerUrl=https://portal.example/ref?token=SINTETICO"
            ),
            Some("https://portal.example".into())
        );
    }

    #[test]
    fn prefiere_hosturl_sobre_referrer() {
        let ini = "[ZoneTransfer]\r\nZoneId=3\r\n\
                   ReferrerUrl=https://banco.example/area\r\n\
                   HostUrl=https://banco.example/nomina.pdf\r\n";
        assert_eq!(
            parse_zone_identifier(ini).as_deref(),
            Some("https://banco.example")
        );
    }

    #[test]
    fn cae_a_referrer_si_no_hay_hosturl() {
        let ini = "[ZoneTransfer]\r\nZoneId=3\r\nReferrerUrl=https://consulado.example/visados\r\n";
        assert_eq!(
            parse_zone_identifier(ini).as_deref(),
            Some("https://consulado.example")
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
        assert_eq!(
            parse_zone_identifier("[ZoneTransfer]\r\nZoneId=3\r\n"),
            None
        );
        assert_eq!(parse_zone_identifier(""), None);
    }
}
