//! Lo que un documento lleva dentro, visto desde la interfaz ([ADR-0030]).
//!
//! La detección está en `obrain-link` ([`obrain_link::dentro`]); aquí sólo se
//! lee lo que dejó en el log y se dice en palabras de persona: qué contiene un
//! paquete, dónde está cada cosa en su texto, y qué paquetes hay.
//!
//! [ADR-0030]: ../../../docs/adr/0030-un-documento-dentro-de-otro.md

use obrain_core::{Assertion, EntityId, PART_OF, Result, Store, TimePoint};

/// Un documento dentro de otro.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Parte {
    /// El documento de dentro.
    pub id: String,
    pub label: String,
    /// Dónde está en el texto del paquete, en bytes: lo que marca el
    /// «¿por qué?».
    pub span: Option<[usize; 2]>,
    /// Párrafos suyos que están en el paquete de un tirón…
    pub paragraphs: usize,
    /// …de los que cuentan.
    pub of: usize,
    pub sensitivity: &'static str,
    /// Está en la papelera: se sigue diciendo que estaba dentro (ADR-0025).
    pub trashed: bool,
}

/// Un paquete: un documento que lleva otros dentro.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Paquete {
    pub id: String,
    pub label: String,
    pub path: Option<String>,
    pub sensitivity: &'static str,
    pub parts: Vec<Parte>,
}

/// `parrafos=89/89` → `(89, 89)`.
fn parrafos(evidencia: &str) -> (usize, usize) {
    evidencia
        .split_whitespace()
        .find_map(|p| p.strip_prefix("parrafos="))
        .and_then(|v| v.split_once('/'))
        .and_then(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?)))
        .unwrap_or((0, 0))
}

/// El tramo que dice la evidencia de un `part_of`, en el texto del paquete.
pub fn tramo(a: &Assertion) -> Option<[usize; 2]> {
    let (x, y) = obrain_link::dentro::tramo(a.evidence.as_deref()?)?;
    Some([x, y])
}

fn parte(store: &Store, a: &Assertion, at: TimePoint) -> Result<Parte> {
    let (paragraphs, of) = parrafos(a.evidence.as_deref().unwrap_or(""));
    Ok(Parte {
        id: a.subject.0.clone(),
        label: obrain_link::label(store, &a.subject, at),
        span: tramo(a),
        paragraphs,
        of,
        sensitivity: store
            .sensitivity(&a.subject)
            .map(|s| s.as_str())
            .unwrap_or("vault"),
        trashed: crate::papelera::of(store, &a.subject, at)?.is_some(),
    })
}

/// Lo que lleva dentro este documento, en el orden en que aparece en su
/// texto.
pub fn parts(store: &Store, id: &EntityId, at: TimePoint) -> Result<Vec<Parte>> {
    let mut out = Vec::new();
    for a in store.pointing_at(id, Some(PART_OF), at)? {
        out.push(parte(store, &a, at)?);
    }
    out.sort_by(|x, y| (x.span.map(|s| s[0]), &x.label).cmp(&(y.span.map(|s| s[0]), &y.label)));
    Ok(out)
}

/// Todos los paquetes, por nombre. Es `obrain paquetes`.
pub fn packages(store: &Store, at: TimePoint) -> Result<Vec<Paquete>> {
    let mut ids: Vec<EntityId> = store
        .query(None, Some(PART_OF), at)?
        .into_iter()
        .filter_map(|a| a.object.as_entity().cloned())
        .collect();
    ids.sort_by(|x, y| x.as_str().cmp(y.as_str()));
    ids.dedup();
    let mut out = Vec::new();
    for id in ids {
        out.push(Paquete {
            label: obrain_link::label(store, &id, at),
            path: store
                .resolve(&id, "path_observed_at", at)?
                .and_then(|a| a.object.as_text().map(str::to_string)),
            sensitivity: store
                .sensitivity(&id)
                .map(|s| s.as_str())
                .unwrap_or("vault"),
            parts: parts(store, &id, at)?,
            id: id.0,
        });
    }
    out.sort_by_key(|p| p.label.to_lowercase());
    Ok(out)
}

/// El orden de una lista de resultados con cada documento **justo debajo del
/// paquete que lo lleva**, si el paquete también salió (ADR-0030 §3).
///
/// `ids` va en el orden de la búsqueda; `paquetes` dice, de cada uno, en qué
/// paquetes está. Devuelve el orden nuevo, como posiciones de `ids`, y para
/// cada posición nueva el paquete bajo el que va. Ninguna fila se esconde: el
/// grupo sube al puesto del mejor de los suyos, y dentro de él cada uno
/// conserva su orden. Si un documento está en dos paquetes de la lista, va
/// con el que salió antes.
pub fn agrupar(
    ids: &[String],
    paquetes: &std::collections::HashMap<String, Vec<String>>,
) -> Vec<(usize, Option<String>)> {
    use std::collections::HashMap;
    let puesto: HashMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, e)| (e.as_str(), i))
        .collect();
    // El paquete de cada uno: el mejor colocado de los que salieron.
    let padre: Vec<Option<usize>> = ids
        .iter()
        .map(|e| {
            paquetes
                .get(e)
                .into_iter()
                .flatten()
                .filter_map(|p| puesto.get(p.as_str()).copied())
                .min()
        })
        .collect();
    let mut hijos: Vec<Vec<usize>> = vec![Vec::new(); ids.len()];
    for (i, p) in padre.iter().enumerate() {
        if let Some(p) = p {
            hijos[*p].push(i);
        }
    }
    // El puesto de un grupo: el del mejor de los suyos.
    fn mejor(i: usize, hijos: &[Vec<usize>], vistos: &mut Vec<bool>) -> usize {
        if std::mem::replace(&mut vistos[i], true) {
            return i;
        }
        hijos[i]
            .iter()
            .map(|&h| mejor(h, hijos, vistos))
            .fold(i, usize::min)
    }
    fn poner(
        i: usize,
        hijos: &[Vec<usize>],
        ids: &[String],
        padre: Option<usize>,
        out: &mut Vec<(usize, Option<String>)>,
        puestos: &mut Vec<bool>,
    ) {
        if std::mem::replace(&mut puestos[i], true) {
            return;
        }
        out.push((i, padre.map(|p| ids[p].clone())));
        for &h in &hijos[i] {
            poner(h, hijos, ids, Some(i), out, puestos);
        }
    }
    let mut raices: Vec<(usize, usize)> = (0..ids.len())
        .filter(|&i| padre[i].is_none())
        .map(|i| (mejor(i, &hijos, &mut vec![false; ids.len()]), i))
        .collect();
    raices.sort_unstable();
    let mut out = Vec::with_capacity(ids.len());
    let mut puestos = vec![false; ids.len()];
    for (_, r) in raices {
        poner(r, &hijos, ids, None, &mut out, &mut puestos);
    }
    // Un ciclo no puede darse (el paquete siempre es más largo que lo suyo),
    // pero si se diera, nada se pierde: lo que falte va al final, tal cual.
    for (i, puesto) in puestos.iter().enumerate() {
        if !puesto {
            out.push((i, None));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{agrupar, parrafos};
    use std::collections::HashMap;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn orden(v: &[&str], dentro: &[(&str, &str)]) -> Vec<(String, Option<String>)> {
        let ids = ids(v);
        let mut paquetes: HashMap<String, Vec<String>> = HashMap::new();
        for (parte, paquete) in dentro {
            paquetes
                .entry(parte.to_string())
                .or_default()
                .push(paquete.to_string());
        }
        agrupar(&ids, &paquetes)
            .into_iter()
            .map(|(i, p)| (ids[i].clone(), p))
            .collect()
    }

    fn so(s: &str) -> Option<String> {
        Some(s.to_string())
    }

    /// El estado de cuenta salió primero y el paquete cuarto: el grupo sube
    /// al primer puesto, con el paquete arriba y el suyo debajo.
    #[test]
    fn lo_de_dentro_va_debajo_de_su_paquete_y_el_grupo_sube() {
        let v = orden(
            &["marzo", "matricula", "abril", "visa"],
            &[("marzo", "visa"), ("abril", "visa")],
        );
        assert_eq!(
            v,
            vec![
                ("visa".into(), None),
                ("marzo".into(), so("visa")),
                ("abril".into(), so("visa")),
                ("matricula".into(), None),
            ]
        );
    }

    /// Si el paquete no salió, nada se mueve y nadie va metido.
    #[test]
    fn sin_su_paquete_en_la_lista_no_se_mueve() {
        let v = orden(&["marzo", "matricula"], &[("marzo", "visa")]);
        assert_eq!(v, vec![("marzo".into(), None), ("matricula".into(), None)]);
    }

    /// En dos paquetes de la lista: con el que salió antes. Y no se repite.
    #[test]
    fn en_dos_paquetes_va_con_el_mejor_colocado() {
        let v = orden(
            &["respaldo", "visa", "marzo"],
            &[("marzo", "visa"), ("marzo", "respaldo")],
        );
        assert_eq!(
            v,
            vec![
                ("respaldo".into(), None),
                ("marzo".into(), so("respaldo")),
                ("visa".into(), None),
            ]
        );
    }

    #[test]
    fn ninguna_fila_se_pierde() {
        let v = orden(
            &["a", "b", "c", "d", "e"],
            &[("b", "d"), ("c", "d"), ("e", "a")],
        );
        let mut vistos: Vec<String> = v.iter().map(|(e, _)| e.clone()).collect();
        vistos.sort();
        assert_eq!(vistos, ids(&["a", "b", "c", "d", "e"]));
    }

    #[test]
    fn los_parrafos_se_leen_de_la_evidencia() {
        assert_eq!(parrafos("parrafos=89/91 bytes=1..2"), (89, 91));
        assert_eq!(parrafos("bytes=1..2"), (0, 0));
    }
}
