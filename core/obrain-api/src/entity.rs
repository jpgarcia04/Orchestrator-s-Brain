//! El panel de entidad de `EXPERIENCE.md` §4.4, resuelto en el núcleo.
//!
//! El panel tiene un orden fijo y no es estético: **qué es, lo observado, lo
//! inferido, la corrección**. La separación entre los dos bloques centrales es
//! el invariante I9 hecho pantalla — un hecho determinista y una sospecha de un
//! clasificador no pueden leerse igual — y por eso la decide este módulo y no
//! una hoja de estilos.

use std::collections::HashMap;

use obrain_core::{Assertion, EntityId, Method, Result, Store, TimePoint, Value};

/// Una afirmación sobre la entidad, lista para enseñarse.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Claim {
    /// Identificador de la aserción. Es lo que permite corregirla o retractarla
    /// de forma concreta en vez de "borrar esta línea".
    pub assertion: String,
    pub predicate: String,
    /// La etiqueta del vocabulario controlado: `contained_by` → "contenido en".
    pub label: String,
    /// El objeto ya renderizado.
    pub value: String,
    /// Si el objeto es otra entidad, su identificador — para poder navegar
    /// hasta ella sin volver a consultar.
    pub value_entity: Option<String>,
    /// Si el objeto es un instante, en epoch ms. `value` ya trae la fecha
    /// escrita, pero en UTC: la hora de verdad es la de quien mira, y la zona
    /// horaria sólo la conoce la ventana.
    pub value_time: Option<i64>,
    pub method: &'static str,
    /// → **saturación**. Una inferencia dudosa sale literalmente pálida.
    pub confidence: f64,
    /// Quién lo afirmó: `linker:hash`, `classifier@v1`, `scan@m1`…
    pub source: String,
    /// El fragmento exacto del origen. Es la respuesta literal a "¿por qué
    /// crees eso?" y la razón de que `evidence` esté en el modelo desde M0.
    /// En una respuesta tuya que confirma lo que se leyó, la de esa lectura:
    /// la fila es una, y su «¿por qué?» sigue ahí.
    pub evidence: Option<String>,
    /// La afirmó el usuario. Precedencia permanente (I5) y no vuelve a cambiar.
    pub by_user: bool,
    /// El tramo del documento que lo explica, en bytes del texto entero: lo
    /// que el «¿por qué?» marca en la vista previa. Lo tienen la fecha de
    /// caducidad (su evidencia lo dice) y la sensibilidad que salió del
    /// contenido, que se **recalcula** aquí sobre el fragmento que dice su
    /// evidencia: el log sólo guarda la dirección, nunca lo encontrado.
    pub span: Option<[usize; 2]>,
    /// El tramo en el texto **del objeto**: dónde está este documento dentro
    /// del paquete de `part_of` (ADR-0030). Es a donde lleva «Está dentro de».
    pub target_span: Option<[usize; 2]>,
}

/// Todo lo que el sistema sabe de una entidad, ordenado como se enseña.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EntityView {
    pub id: String,
    pub label: String,
    pub path: Option<String>,
    /// El "tipo" es esto: una composición de facetas, no una etiqueta rígida.
    pub facets: Vec<String>,
    pub sensitivity: &'static str,
    /// La carpeta marcada que decide ese nivel, si es una carpeta quien lo
    /// decide ([ADR-0020]). Es el «¿por qué?» de lo que no tiene una aserción
    /// propia: la foto de la TIE es sensible porque está en `01 Identidad`.
    ///
    /// [ADR-0020]: ../../../docs/adr/0020-sensibilidad-por-carpeta.md
    pub protection: Option<crate::Protection>,
    /// El documento que lleva dentro y que decide ese nivel, si es él quien
    /// lo decide ([ADR-0030]): el PDF de la visa es sensible porque contiene
    /// la carátula del pasaporte.
    ///
    /// [ADR-0030]: ../../../docs/adr/0030-un-documento-dentro-de-otro.md
    pub by_part: Option<crate::marks::ByPart>,
    /// Lo que lleva dentro, en el orden de su texto (ADR-0030).
    pub parts: Vec<crate::dentro::Parte>,
    /// Hechos. Deterministas o afirmados por el usuario. **Bloque sólido.**
    pub observed: Vec<Claim>,
    /// Sospechas. Heurísticas, clasificadores, importaciones sin verificar.
    /// **Bloque desaturado, y cada línea corregible.**
    pub inferred: Vec<Claim>,
    /// Está en la papelera: ya no está en tu bóveda ([ADR-0025]). Dónde
    /// estaba y cuánto le queda.
    ///
    /// [ADR-0025]: ../../../docs/adr/0025-lo-que-ya-no-esta.md
    pub trashed: Option<crate::EnPapelera>,
    /// Las colecciones donde lo metiste (ADR-0031).
    pub collections: Vec<crate::Pertenece>,
}

/// Predicados que no son información sobre la entidad sino contabilidad interna
/// del pipeline. Enseñarlos en el panel llenaría de ruido justo el sitio donde
/// el usuario tiene que poder confiar de un vistazo.
const PLUMBING: &[&str] = &[
    "has_facet",
    "path_observed_at",
    "file_id",
    "chunker",
    "text_hash",
    "chunk_count",
    "text_chars",
    "vector_count",
    // La papelera se enseña aparte, como aviso y no como un hecho más.
    "missing_since",
    "forgotten_at",
    // Las colecciones también, como chips junto a su carpeta (ADR-0031).
    "member_of",
    "collection_name",
];

/// El objeto, en palabras. Un instante sale como fecha y no como
/// `1789839880208`, y un tamaño en bytes como `1,2 MB`.
fn render(store: &Store, a: &Assertion, at: TimePoint) -> (String, Option<String>, Option<i64>) {
    match &a.object {
        Value::Entity(e) => (obrain_link::label(store, e, at), Some(e.0.clone()), None),
        Value::Text(t) => (t.clone(), None, None),
        Value::Number(n) if a.predicate == "file_size" => (tamano(*n), None, None),
        Value::Number(n) => (format!("{n}"), None, None),
        Value::Timestamp(t) => (obrain_link::fecha(*t), None, Some(*t)),
        Value::Bool(b) => (b.to_string(), None, None),
    }
}

/// Bytes → `74 B`, `1,2 MB`. Con coma decimal: es lo que lee quien lo mira.
fn tamano(bytes: f64) -> String {
    const UNIDADES: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes.max(0.0);
    let mut u = 0;
    while v >= 1024.0 && u < UNIDADES.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{v:.0} B")
    } else {
        format!("{v:.1} {}", UNIDADES[u]).replace('.', ",")
    }
}

/// Reúne lo que se sabe de una entidad y lo parte en observado e inferido.
///
/// El corte es por método y no por confianza: `Deterministic` y `User` son
/// hechos —un hash y una corrección del usuario lo son por motivos distintos y
/// los dos lo son—, y todo lo demás es una sospecha con procedencia. Cortar por
/// `confidence == 1.0` daría casi el mismo resultado hoy y sería frágil: basta
/// que un clasificador se declare seguro para colarse en el bloque sólido.
pub fn view(store: &Store, id: &EntityId, at: TimePoint) -> Result<EntityView> {
    let all = store.query(Some(id), None, at)?;

    // Lo que ya contestaste (I5): de cada dato —la fecha de caducidad, la
    // sensibilidad; no las relaciones— manda tu última respuesta, y es la
    // única fila que se enseña. `query` ordena por precedencia: es la primera
    // `user` de su predicado. Sin esto, cada «es correcto» era una fila más y
    // la inferencia confirmada seguía debajo pidiendo que la confirmaras.
    let mut respuestas: HashMap<&str, &Assertion> = HashMap::new();
    for a in &all {
        if a.method == Method::User && !matches!(a.object, Value::Entity(_)) {
            respuestas.entry(a.predicate.as_str()).or_insert(a);
        }
    }

    let mut observed = Vec::new();
    let mut inferred = Vec::new();
    for a in &all {
        if PLUMBING.contains(&a.predicate.as_str()) {
            continue;
        }
        // Lo que explica la fila. La tuya, si confirmaste lo que leí, se
        // queda con el «¿por qué?» de la lectura: decir que sí no cambia la
        // razón.
        let mut porque = a;
        if let Some(u) = respuestas.get(a.predicate.as_str()) {
            if u.id != a.id {
                continue;
            }
            if let Some(leido) = all.iter().find(|x| {
                x.method != Method::User && x.predicate == a.predicate && x.object == a.object
            }) {
                porque = leido;
            }
        }
        let (value, value_entity, value_time) = render(store, a, at);
        let claim = Claim {
            assertion: a.id.0.clone(),
            label: store
                .predicate_label(&a.predicate)?
                .unwrap_or_else(|| a.predicate.clone()),
            predicate: a.predicate.clone(),
            value,
            value_entity,
            value_time,
            method: a.method.as_str(),
            confidence: a.confidence,
            source: a.source.clone(),
            evidence: porque.evidence.clone(),
            by_user: a.method == Method::User,
            span: span(store, id, porque),
            target_span: (a.predicate == obrain_core::PART_OF)
                .then(|| crate::dentro::tramo(a))
                .flatten(),
        };
        match a.method {
            Method::Deterministic | Method::User => observed.push(claim),
            Method::Inferred | Method::Imported => inferred.push(claim),
        }
    }

    let (protection, by_part) = crate::marks::deciding(store, id, at)?;
    Ok(EntityView {
        label: obrain_link::label(store, id, at),
        path: store
            .resolve(id, "path_observed_at", at)?
            .and_then(|a| a.object.as_text().map(str::to_string)),
        facets: store.facets(id, at)?,
        sensitivity: store.sensitivity(id).map(|s| s.as_str()).unwrap_or("vault"),
        protection,
        by_part,
        parts: crate::dentro::parts(store, id, at)?,
        observed,
        inferred,
        trashed: crate::papelera::of(store, id, at)?,
        collections: crate::colecciones::de(store, id, at)?,
        id: id.0.clone(),
    })
}

/// El tramo que explica una aserción, si se sabe. Ver [`Claim::span`].
fn span(store: &Store, id: &EntityId, a: &Assertion) -> Option<[usize; 2]> {
    let ev = a.evidence.as_deref()?;
    let campo = |k: &str| {
        ev.split_whitespace()
            .find_map(|p| p.strip_prefix(k).map(str::to_string))
    };
    match a.predicate.as_str() {
        "expires_on" => {
            let (x, y) = campo("bytes=")?
                .split_once("..")
                .map(|(x, y)| (x.to_string(), y.to_string()))?;
            Some([x.parse().ok()?, y.parse().ok()?])
        }
        "sensitivity_level" => {
            let regla = campo("regla=")?;
            let fragmento: i64 = campo("fragmento=")?.parse().ok()?;
            let (texto, base): (String, i64) = store
                .conn()
                .query_row(
                    "SELECT text, byte_start FROM chunks WHERE entity = ?1 AND ordinal = ?2
                     ORDER BY chunker_ver DESC LIMIT 1",
                    obrain_core::rusqlite::params![id.as_str(), fragmento],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .ok()?;
            let (s, r) = obrain_guard::content::locate(&texto)?;
            // Si el texto de hoy dispara otra regla, el tramo no explicaría
            // esta aserción: mejor no marcar nada que marcar otra cosa.
            (s.rule == regla).then(|| {
                let b = base.max(0) as usize;
                [b + r.start, b + r.end]
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::tamano;

    #[test]
    fn un_tamano_se_lee_como_lo_diria_una_persona() {
        assert_eq!(tamano(74.0), "74 B");
        assert_eq!(tamano(1536.0), "1,5 KB");
        assert_eq!(tamano(5.4 * 1024.0 * 1024.0), "5,4 MB");
    }
}
