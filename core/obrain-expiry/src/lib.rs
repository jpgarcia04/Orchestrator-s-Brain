//! # obrain-expiry — vencimientos · Fase 3.4
//!
//! *¿Qué me caduca pronto?* La fecha ya está escrita en el documento —«VALIDEZ
//! 15 11 2026» en la TIE, «vigencia hasta el 31 de agosto de 2027» en la
//! póliza— y desde el OCR (ADR-0022) también en las fotos. Esto la lee del
//! texto que M2 ya dejó en `chunks` y la afirma como **una aserción inferida
//! por documento**, `expires_on`, corregible por el usuario (I5).
//!
//! ```text
//!   escanear → enlazar → extraer → clasificar → VENCIMIENTOS → vectorizar
//! ```
//!
//! No abre archivos, igual que la puerta: lee la proyección léxica. Y como la
//! puerta, **la evidencia es una dirección y nunca una cita**: el log es
//! inmutable (I1) y una palabra del pasaporte copiada ahí se quedaría para
//! siempre. El porqué de cada decisión, en
//! [ADR-0023](../../../docs/adr/0023-vencimientos.md).

pub mod fecha;
pub mod texto;

use obrain_core::rusqlite::Connection;
use obrain_core::{
    Assertion, AssertionId, EntityId, Method, NewAssertion, Result, Store, TimePoint, Value,
};
use std::collections::{BTreeMap, HashMap};

pub use fecha::Fecha;
pub use texto::{Candidato, Regla};

/// El predicado. Cardinalidad `one`: un documento caduca una vez.
pub const PREDICATE: &str = "expires_on";

/// Identidad del detector. Va en `source`, y es lo que le permite retractar
/// **sólo lo suyo**: nunca una corrección del usuario.
pub const DETECTOR: &str = "expiry";
pub const DETECTOR_VERSION: i64 = 1;

/// Lo que escribe el usuario cuando dice que un documento **no caduca**.
pub const NUNCA: &str = "nunca";

/// Con cuántos días de antelación avisa la tarjeta «Caduca pronto». Lo eligió
/// el usuario (ADR-0023): 90, igual para todo.
pub const VENTANA_DIAS: i64 = 90;

/// El `source` de lo que escribe el detector.
pub fn source() -> String {
    format!("{DETECTOR}@v{DETECTOR_VERSION}")
}

// ─────────────────────────────────────────────────────────────── elegir

/// La fecha elegida para un documento, con todo lo que hace falta para
/// explicarla.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Eleccion {
    pub fecha: Fecha,
    pub regla: Regla,
    /// Ordinal del fragmento donde está.
    pub fragmento: i64,
    /// `[inicio, fin)` en bytes **del documento entero**.
    pub inicio: usize,
    pub fin: usize,
    /// Cuántas otras fechas candidatas había y se descartaron.
    pub otras: usize,
}

impl Eleccion {
    /// **Dónde** está y por qué se eligió; nunca **qué** dice el documento.
    pub fn evidence(&self) -> String {
        let mut e = format!(
            "regla={} fragmento={} bytes={}..{}",
            self.regla.as_str(),
            self.fragmento,
            self.inicio,
            self.fin
        );
        if self.otras > 0 {
            e.push_str(&format!(" otras={}", self.otras));
        }
        e
    }
}

/// Una candidata situada en el documento: ordinal del fragmento y offsets
/// del documento entero.
#[derive(Debug, Clone, Copy)]
pub struct Situada {
    pub fragmento: i64,
    pub candidato: Candidato,
}

/// **Una por documento**: la de la palabra más clara y, si empatan, la más
/// lejana, que suele ser la vigencia del propio documento (ADR-0023 §3).
///
/// La misma fecha vista dos veces —dos palabras, o dos fragmentos que se
/// solapan— es una sola candidata, con la mejor de sus palabras y su primera
/// aparición.
pub fn elegir(candidatas: &[Situada]) -> Option<Eleccion> {
    let mut por_fecha: BTreeMap<Fecha, Situada> = BTreeMap::new();
    for s in candidatas {
        let c = s.candidato;
        por_fecha
            .entry(c.fecha)
            .and_modify(|v| {
                let mejor = (c.regla.peso(), std::cmp::Reverse(c.inicio))
                    > (
                        v.candidato.regla.peso(),
                        std::cmp::Reverse(v.candidato.inicio),
                    );
                if mejor {
                    *v = *s;
                }
            })
            .or_insert(*s);
    }
    let otras = por_fecha.len().saturating_sub(1);
    let ganadora = por_fecha
        .values()
        .max_by_key(|s| (s.candidato.regla.peso(), s.candidato.fecha))?;
    Some(Eleccion {
        fecha: ganadora.candidato.fecha,
        regla: ganadora.candidato.regla,
        fragmento: ganadora.fragmento,
        inicio: ganadora.candidato.inicio,
        fin: ganadora.candidato.fin,
        otras,
    })
}

// ─────────────────────────────────────────────────────────────── detectar

/// Lo que hizo una pasada.
#[derive(Debug, Default, Clone)]
pub struct ExpiryReport {
    /// Documentos con texto que se miraron.
    pub with_text: usize,
    /// De ésos, en cuántos se encontró una fecha de caducidad.
    pub found: usize,
    pub assertions_written: usize,
    pub assertions_unchanged: usize,
    pub assertions_retracted: usize,
    pub by_rule: BTreeMap<&'static str, usize>,
}

/// Las candidatas de cada documento, leídas de los fragmentos de M2.
///
/// Una sola consulta para todo, como la puerta. Los offsets pasan de ser del
/// fragmento a ser del documento sumando `byte_start`: un fragmento **es** la
/// rebanada `[byte_start, byte_end)` del documento (ADR-0005).
fn candidatas(conn: &Connection) -> Result<BTreeMap<EntityId, Vec<Situada>>> {
    let mut st = conn
        .prepare("SELECT entity, ordinal, byte_start, text FROM chunks ORDER BY entity, ordinal")?;
    let mut rows = st.query([])?;
    let mut out: BTreeMap<EntityId, Vec<Situada>> = BTreeMap::new();
    while let Some(row) = rows.next()? {
        let entity = EntityId(row.get(0)?);
        let fragmento: i64 = row.get(1)?;
        let base = row.get::<_, i64>(2)?.max(0) as usize;
        let texto: String = row.get(3)?;
        let lista = out.entry(entity).or_default();
        for c in texto::scan(&texto) {
            lista.push(Situada {
                fragmento,
                candidato: Candidato {
                    inicio: base + c.inicio,
                    fin: base + c.fin,
                    ..c
                },
            });
        }
    }
    Ok(out)
}

/// Lo que el detector afirmó antes y sigue vigente, por documento. **Sólo lo
/// suyo**: lo del usuario no se toca.
fn previas(store: &Store, tp: TimePoint) -> Result<HashMap<EntityId, Vec<Assertion>>> {
    let mut m: HashMap<EntityId, Vec<Assertion>> = HashMap::new();
    for a in store.query(None, Some(PREDICATE), tp)? {
        if a.method != Method::User && a.source.starts_with(DETECTOR) {
            m.entry(a.subject.clone()).or_default().push(a);
        }
    }
    Ok(m)
}

/// **Busca vencimientos en todo lo que tiene texto.** Idempotente: si un
/// documento dice lo mismo que la vez anterior, no escribe nada.
///
/// Si un documento ya no dice nada —o dice otra fecha—, retracta lo que el
/// detector escribió de él (I4: retractar, no borrar) y afirma lo nuevo. Lo
/// que escribió el usuario **no lo toca nunca**: por eso una corrección
/// sobrevive a cualquier reindexado (I5).
pub fn detect_all(store: &Store, at: i64, report: &mut ExpiryReport) -> Result<()> {
    let tp = TimePoint::at(at);
    let todas = candidatas(store.conn())?;
    let mut antes = previas(store, tp)?;
    report.with_text = todas.len();

    for (entity, lista) in &todas {
        let previas = antes.remove(entity).unwrap_or_default();
        let Some(e) = elegir(lista) else {
            retractar(store, &previas, at, report)?;
            continue;
        };
        report.found += 1;
        *report.by_rule.entry(e.regla.as_str()).or_default() += 1;

        let valor = e.fecha.iso();
        let evidencia = e.evidence();
        let ya_dicho = previas.iter().any(|a| {
            a.object.as_text() == Some(valor.as_str()) && a.evidence.as_deref() == Some(&evidencia)
        });
        if ya_dicho {
            report.assertions_unchanged += 1;
            continue;
        }
        retractar(store, &previas, at, report)?;
        store.assert(
            NewAssertion::new(
                entity.clone(),
                PREDICATE,
                Value::Text(valor),
                Method::Inferred,
                at,
            )
            .source(&source())
            .confidence(e.regla.confianza())
            .evidence(&evidencia),
        )?;
        report.assertions_written += 1;
    }

    // Lo que tenía fecha y ya no tiene texto: el texto se fue, la inferencia
    // también.
    for previas in antes.values() {
        retractar(store, previas, at, report)?;
    }
    Ok(())
}

fn retractar(
    store: &Store,
    previas: &[Assertion],
    at: i64,
    report: &mut ExpiryReport,
) -> Result<()> {
    for a in previas {
        store.retract(&a.id, at)?;
        report.assertions_retracted += 1;
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────── leer

/// Qué se sabe de cuándo caduca un documento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vence {
    El(Fecha),
    /// El usuario dijo que no caduca.
    Nunca,
}

impl Vence {
    pub fn parse(s: &str) -> Option<Self> {
        if s == NUNCA {
            Some(Vence::Nunca)
        } else {
            Fecha::parse_iso(s).map(Vence::El)
        }
    }

    pub fn fecha(self) -> Option<Fecha> {
        match self {
            Vence::El(f) => Some(f),
            Vence::Nunca => None,
        }
    }
}

/// El vencimiento **resuelto** de un documento: el del usuario si lo hay.
#[derive(Debug, Clone)]
pub struct Vencimiento {
    pub entity: EntityId,
    pub vence: Vence,
    pub by_user: bool,
    pub confidence: f64,
    pub evidence: Option<String>,
}

/// El vencimiento resuelto de cada documento que tiene uno.
///
/// La precedencia es la de siempre: `Store::query` devuelve las aserciones
/// ordenadas por método —el usuario primero (I5)— y luego por la más
/// reciente, así que la primera de cada documento es la que manda.
pub fn resolved(store: &Store, tp: TimePoint) -> Result<Vec<Vencimiento>> {
    let mut vistos = std::collections::HashSet::new();
    let mut out = Vec::new();
    for a in store.query(None, Some(PREDICATE), tp)? {
        if !vistos.insert(a.subject.clone()) {
            continue;
        }
        let Some(vence) = a.object.as_text().and_then(Vence::parse) else {
            continue;
        };
        out.push(Vencimiento {
            entity: a.subject,
            vence,
            by_user: a.method == Method::User,
            confidence: a.confidence,
            evidence: a.evidence,
        });
    }
    Ok(out)
}

/// **La palabra del usuario**: esta fecha, o `None` para «no caduca». Gana
/// para siempre (I5). No retracta la inferencia que corrige: conservarla es lo
/// que permite decir después qué creía el sistema y en qué se equivocó, igual
/// que `Brain::correct`.
pub fn set_by_user(
    store: &Store,
    entity: &EntityId,
    fecha: Option<Fecha>,
    at: i64,
) -> Result<AssertionId> {
    let valor = fecha.map_or_else(|| NUNCA.to_string(), |f| f.iso());
    store.assert(
        NewAssertion::new(
            entity.clone(),
            PREDICATE,
            Value::Text(valor),
            Method::User,
            at,
        )
        .source("user@ui"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(fragmento: i64, iso: &str, regla: Regla, inicio: usize) -> Situada {
        Situada {
            fragmento,
            candidato: Candidato {
                fecha: Fecha::parse_iso(iso).unwrap(),
                regla,
                inicio,
                fin: inicio + 10,
            },
        }
    }

    #[test]
    fn gana_la_palabra_mas_clara_y_luego_la_mas_lejana() {
        let e = elegir(&[
            s(0, "2026-12-31", Regla::Hasta, 10),
            s(1, "2027-08-31", Regla::Vigencia, 500),
            s(1, "2027-08-31", Regla::Hasta, 520),
            s(2, "2026-11-15", Regla::Vigencia, 900),
        ])
        .unwrap();
        assert_eq!(e.fecha.iso(), "2027-08-31");
        assert_eq!(e.regla, Regla::Vigencia);
        assert_eq!(e.inicio, 500);
        assert_eq!(e.otras, 2);

        // Una palabra más clara gana aunque su fecha sea más cercana.
        let e = elegir(&[
            s(0, "2030-01-01", Regla::Vigencia, 0),
            s(0, "2026-11-15", Regla::Validez, 40),
        ])
        .unwrap();
        assert_eq!(e.fecha.iso(), "2026-11-15");
    }

    #[test]
    fn la_misma_fecha_en_dos_fragmentos_cuenta_una_vez() {
        let e = elegir(&[
            s(0, "2026-11-15", Regla::Validez, 1_000),
            s(1, "2026-11-15", Regla::Validez, 1_000),
        ])
        .unwrap();
        assert_eq!(e.otras, 0);
        assert_eq!(e.fragmento, 0);
        assert_eq!(e.evidence(), "regla=validez fragmento=0 bytes=1000..1010");
    }

    #[test]
    fn sin_candidatas_no_hay_eleccion() {
        assert!(elegir(&[]).is_none());
    }
}
