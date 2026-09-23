//! `part_of` — un documento dentro de otro ([ADR-0030]).
//!
//! El PDF que se arma para un trámite juntando otros PDF —los estados de
//! cuenta, la carátula del pasaporte— **contiene** documentos que también
//! están sueltos. Sin decirlo, una búsqueda devuelve cinco filas donde hay tres
//! documentos, y nadie sabe por qué se parecen tanto.
//!
//! ## Párrafos, no fragmentos
//!
//! El fragmento del chunker junta párrafos y solapa con el anterior: en las
//! orillas de cada documento metido en un paquete se mezcla con el texto del
//! vecino y ya no es idéntico. Medido sobre la base de agosto, por fragmentos
//! se encontraba **uno** de los 14 casos; por párrafos, los 14. El párrafo es
//! el del chunker ([`obrain_extract::chunk::paragraphs`]), con los espacios
//! normalizados, y sólo cuentan los de [`MIN_PARRAFO`] bytes o más: «Página 3
//! de 20» cambia de un PDF a otro y no dice nada.
//!
//! ## De un tirón, y con algo más
//!
//! B está dentro de A si sus párrafos aparecen **seguidos y en el mismo
//! orden** en A ([`DE_UN_TIRON`]) —lo que sobre, sólo en las orillas— y A trae
//! algo más que B ([`MAX_DE_A`]). Eso es lo que separa un paquete de una
//! **versión**: en una versión lo cambiado está repartido por en medio, o es
//! el mismo documento con un añadido. Las versiones siguen congeladas; aquí
//! sólo se cuida de no confundirlas.
//!
//! [ADR-0030]: ../../../docs/adr/0030-un-documento-dentro-de-otro.md

use obrain_core::{EntityId, Method, NewAssertion, Result, Store, TimePoint, Value};
use std::collections::{HashMap, HashSet};

/// El predicado: el de dentro `part_of` el de fuera. Es el mismo que lee la
/// sensibilidad para que el paquete herede (ADR-0030 §4).
pub const PREDICATE: &str = obrain_core::PART_OF;
/// Quién lo afirma. La versión sube si cambia lo que se detecta.
pub const SOURCE: &str = "linker:parrafos@v1";
/// Lo que escribió cualquier versión de este enlazador.
const PROPIO: &str = "linker:parrafos";

/// Un párrafo más corto que esto no cuenta, ni a favor ni en contra.
pub const MIN_PARRAFO: usize = 40;
/// Un documento de una o dos líneas «cabe» en cualquier sitio.
pub const MIN_PARRAFOS: usize = 3;
/// Cuánto de B tiene que estar en A de un tirón.
pub const DE_UN_TIRON: f64 = 0.95;
/// Hasta cuánto de A puede ser B. Más, y es la misma cosa con un añadido.
pub const MAX_DE_A: f64 = 0.90;

type Huella = [u8; 16];

/// Un párrafo de un documento, reducido a lo que se compara.
#[derive(Debug, Clone)]
struct Parrafo {
    huella: Huella,
    /// Dónde está en el texto entero, en bytes: lo que marca el «¿por qué?».
    inicio: usize,
    fin: usize,
    /// Bytes del párrafo normalizado. Es lo que pesa al comparar.
    peso: usize,
    /// Su puesto entre **todos** los párrafos del documento, cortos incluidos.
    pos: usize,
}

/// Un documento listo para comparar.
struct Doc {
    entity: EntityId,
    /// Los párrafos que cuentan: los de [`MIN_PARRAFO`] bytes o más.
    parrafos: Vec<Parrafo>,
    /// Todos, cortos incluidos: `(huella, inicio, fin)`. Sólo sirven para que
    /// el tramo marcado no se deje fuera el título de un documento
    /// («MOVIMIENTOS DE MARZO») en su orilla.
    todos: Vec<(Huella, usize, usize)>,
    /// Bytes del texto entero.
    bytes: usize,
    /// Suma del peso de sus párrafos.
    peso: usize,
}

fn doc(entity: EntityId, texto: &str) -> Doc {
    let mut parrafos = Vec::new();
    let mut todos = Vec::new();
    for (pos, (inicio, fin)) in obrain_extract::chunk::paragraphs(texto)
        .into_iter()
        .enumerate()
    {
        let norma = texto[inicio..fin]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let mut huella = [0u8; 16];
        huella.copy_from_slice(&blake3::hash(norma.as_bytes()).as_bytes()[..16]);
        todos.push((huella, inicio, fin));
        if norma.len() >= MIN_PARRAFO {
            parrafos.push(Parrafo {
                huella,
                inicio,
                fin,
                peso: norma.len(),
                pos,
            });
        }
    }
    Doc {
        entity,
        bytes: texto.len(),
        peso: parrafos.iter().map(|p| p.peso).sum(),
        parrafos,
        todos,
    }
}

/// Un documento dentro de otro, con lo que lo prueba.
#[derive(Debug, Clone, PartialEq)]
pub struct Dentro {
    /// El de dentro.
    pub parte: EntityId,
    /// El de fuera.
    pub paquete: EntityId,
    /// Párrafos de la parte que están en el paquete de un tirón…
    pub parrafos: usize,
    /// …de los que cuentan en la parte.
    pub de: usize,
    /// Dónde está la parte **en el texto del paquete**, en bytes.
    pub bytes: (usize, usize),
    /// La parte del peso de la parte que está de un tirón (0,95–1,00).
    pub confianza: f64,
}

impl Dentro {
    /// Dónde mirar, sin una palabra del documento (I9).
    pub fn evidencia(&self) -> String {
        format!(
            "parrafos={}/{} bytes={}..{}",
            self.parrafos, self.de, self.bytes.0, self.bytes.1
        )
    }
}

/// ¿Está `b` dentro de `a`? La regla entera de ADR-0030 §2.
fn comparar(a: &Doc, b: &Doc) -> Option<Dentro> {
    if b.parrafos.len() < MIN_PARRAFOS || b.peso == 0 || a.bytes <= b.bytes {
        return None;
    }
    let mut en_a: HashMap<Huella, Vec<usize>> = HashMap::new();
    for (j, p) in a.parrafos.iter().enumerate() {
        en_a.entry(p.huella).or_default().push(j);
    }

    // El tramo más largo —por peso— de párrafos de B que están seguidos y en
    // el mismo orden en A. Sólo se empieza donde empieza un tramo: si el
    // párrafo anterior también casaba, ya se contó desde allí.
    let (mut mejor, mut i0, mut j0, mut largo) = (0usize, 0, 0, 0);
    for (i, p) in b.parrafos.iter().enumerate() {
        for &j in en_a.get(&p.huella).into_iter().flatten() {
            if i > 0 && j > 0 && a.parrafos[j - 1].huella == b.parrafos[i - 1].huella {
                continue;
            }
            let (mut k, mut peso) = (0, 0);
            while i + k < b.parrafos.len()
                && j + k < a.parrafos.len()
                && a.parrafos[j + k].huella == b.parrafos[i + k].huella
            {
                peso += b.parrafos[i + k].peso;
                k += 1;
            }
            if peso > mejor {
                (mejor, i0, j0, largo) = (peso, i, j, k);
            }
        }
    }
    let tiron = mejor as f64 / b.peso as f64;
    if tiron < DE_UN_TIRON || a.peso == 0 || mejor as f64 / a.peso as f64 > MAX_DE_A {
        return None;
    }

    // Las orillas: si el tramo empieza (o acaba) donde empieza (o acaba) B, los
    // párrafos cortos de B de antes (o de después) —un título, una fecha— que
    // estén igual y seguidos en A también son suyos, y se marcan con él. No
    // cuentan para decidir; sólo para que el «¿por qué?» no los deje fuera.
    let mut inicio = a.parrafos[j0].inicio;
    if i0 == 0 {
        let (mut pb, mut pa) = (b.parrafos[0].pos, a.parrafos[j0].pos);
        while pb > 0 && pa > 0 && b.todos[pb - 1].0 == a.todos[pa - 1].0 {
            (pb, pa) = (pb - 1, pa - 1);
            inicio = a.todos[pa].1;
        }
    }
    let ultimo = j0 + largo - 1;
    let mut fin = a.parrafos[ultimo].fin;
    if i0 + largo == b.parrafos.len() {
        let (mut pb, mut pa) = (b.parrafos[i0 + largo - 1].pos, a.parrafos[ultimo].pos);
        while pb + 1 < b.todos.len()
            && pa + 1 < a.todos.len()
            && b.todos[pb + 1].0 == a.todos[pa + 1].0
        {
            (pb, pa) = (pb + 1, pa + 1);
            fin = a.todos[pa].2;
        }
    }

    Some(Dentro {
        parte: b.entity.clone(),
        paquete: a.entity.clone(),
        parrafos: largo,
        de: b.parrafos.len(),
        bytes: (inicio, fin),
        confianza: (tiron * 100.0).floor() / 100.0,
    })
}

/// Todos los pares «B dentro de A» entre estos documentos.
fn buscar(docs: &[Doc]) -> Vec<Dentro> {
    // Huella → documentos que la tienen. Cada documento una vez por huella.
    let mut indice: HashMap<Huella, Vec<usize>> = HashMap::new();
    for (d, doc) in docs.iter().enumerate() {
        let mut vistas = HashSet::new();
        for p in &doc.parrafos {
            if vistas.insert(p.huella) {
                indice.entry(p.huella).or_default().push(d);
            }
        }
    }

    let mut out = Vec::new();
    for (ib, b) in docs.iter().enumerate() {
        if b.parrafos.len() < MIN_PARRAFOS {
            continue;
        }
        // Un primer filtro barato: los A que tienen casi todo B, en cualquier
        // orden. Por huellas distintas, así que es una cota por arriba.
        let mut unicas: HashMap<Huella, usize> = HashMap::new();
        for p in &b.parrafos {
            unicas.insert(p.huella, p.peso);
        }
        let total: usize = unicas.values().sum();
        let mut acumulado: HashMap<usize, usize> = HashMap::new();
        for (h, peso) in &unicas {
            for &ia in indice.get(h).into_iter().flatten() {
                if ia != ib {
                    *acumulado.entry(ia).or_default() += peso;
                }
            }
        }
        let mut candidatos: Vec<usize> = acumulado
            .into_iter()
            .filter(|(_, p)| *p as f64 >= 0.9 * total as f64)
            .map(|(ia, _)| ia)
            .collect();
        candidatos.sort_unstable();
        for ia in candidatos {
            if let Some(d) = comparar(&docs[ia], b) {
                out.push(d);
            }
        }
    }
    out.sort_by(|x, y| {
        (x.paquete.as_str(), x.bytes.0, x.parte.as_str()).cmp(&(
            y.paquete.as_str(),
            y.bytes.0,
            y.parte.as_str(),
        ))
    });
    out
}

/// Lo que hizo una pasada.
#[derive(Debug, Default, Clone)]
pub struct DentroReport {
    /// Documentos con texto que se compararon.
    pub documentos: usize,
    /// Pares «dentro de» que sostiene el texto de hoy.
    pub encontrados: usize,
    pub escritas: usize,
    pub retractadas: usize,
    pub sin_cambio: usize,
}

/// Compara todos los documentos con texto y deja el log diciendo lo que el
/// texto de hoy sostiene (ADR-0030 §2).
///
/// Corre **después** de leer el texto (M2) y **antes** de la puerta, que usa
/// estas aristas para que un paquete herede la sensibilidad de lo que lleva.
/// Lo que está en la papelera ni se compara ni se toca: vuelve con lo que
/// sabía (ADR-0025). Lo olvidado se retracta.
pub fn link(store: &Store, at: i64, report: &mut DentroReport) -> Result<Vec<Dentro>> {
    let tp = TimePoint::at(at);
    let sujetos = |p: &str| -> Result<HashSet<EntityId>> {
        Ok(store
            .query(None, Some(p), tp)?
            .into_iter()
            .map(|a| a.subject)
            .collect())
    };
    let fuera = sujetos(obrain_scan::ausentes::MISSING)?;
    let olvidados = sujetos(obrain_scan::ausentes::FORGOTTEN)?;

    // Por archivo: su fecha y su ruta, para elegir el original de un grupo de
    // textos idénticos con el criterio de `duplicate_of`.
    let archivos: HashMap<EntityId, (i64, String)> = crate::load_files(store, tp)?
        .into_iter()
        .map(|f| (f.entity, (f.modified_ms, f.path)))
        .collect();

    // De cada texto, un documento: el más antiguo. Un paquete no dice
    // «contiene: B, B, B», y una copia llega a él por su «Duplicado de».
    let mut por_texto: HashMap<[u8; 32], Doc> = HashMap::new();
    obrain_extract::index::each_document_text(store.conn(), |entity, texto| {
        if fuera.contains(&entity) || !archivos.contains_key(&entity) {
            return Ok(());
        }
        let clave = *blake3::hash(texto.as_bytes()).as_bytes();
        let gana = match por_texto.get(&clave) {
            Some(otro) => archivos[&entity] < archivos[&otro.entity],
            None => true,
        };
        if gana {
            por_texto.insert(clave, doc(entity, texto));
        }
        Ok(())
    })?;
    let mut docs: Vec<Doc> = por_texto.into_values().collect();
    docs.sort_by(|x, y| x.entity.as_str().cmp(y.entity.as_str()));
    report.documentos = docs.len();

    let hallados = buscar(&docs);
    report.encontrados = hallados.len();

    // Lo que ya estaba dicho: sigue, se retracta o se reafirma con el tramo
    // de hoy. Lo del usuario no se toca (I5).
    let mut nuevos: HashMap<(EntityId, EntityId), &Dentro> = hallados
        .iter()
        .map(|d| ((d.parte.clone(), d.paquete.clone()), d))
        .collect();
    for a in store.query(None, Some(PREDICATE), tp)? {
        if a.method == Method::User || !a.source.starts_with(PROPIO) {
            continue;
        }
        let Some(paquete) = a.object.as_entity().cloned() else {
            continue;
        };
        let en_papelera = |e: &EntityId| fuera.contains(e) && !olvidados.contains(e);
        if en_papelera(&a.subject) || en_papelera(&paquete) {
            continue;
        }
        let clave = (a.subject.clone(), paquete);
        match nuevos.get(&clave) {
            Some(d) if a.evidence.as_deref() == Some(d.evidencia().as_str()) => {
                nuevos.remove(&clave);
                report.sin_cambio += 1;
            }
            _ => {
                store.retract(&a.id, at)?;
                report.retractadas += 1;
            }
        }
    }
    let mut pendientes: Vec<&Dentro> = nuevos.into_values().collect();
    pendientes.sort_by(|x, y| {
        (x.paquete.as_str(), x.parte.as_str()).cmp(&(y.paquete.as_str(), y.parte.as_str()))
    });
    for d in pendientes {
        store.assert(
            NewAssertion::new(
                d.parte.clone(),
                PREDICATE,
                Value::Entity(d.paquete.clone()),
                Method::Deterministic,
                at,
            )
            .source(SOURCE)
            .confidence(d.confianza)
            .evidence(&d.evidencia()),
        )?;
        report.escritas += 1;
    }
    Ok(hallados)
}

/// El tramo que dice una evidencia de `part_of`: `bytes=1234..15678`.
pub fn tramo(evidencia: &str) -> Option<(usize, usize)> {
    let b = evidencia
        .split_whitespace()
        .find_map(|p| p.strip_prefix("bytes="))?;
    let (x, y) = b.split_once("..")?;
    Some((x.parse().ok()?, y.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: i64 = 1_700_000_000_000;

    /// Un párrafo de relleno reconocible, de más de 40 bytes.
    fn p(tema: &str, n: usize) -> String {
        format!("{tema} {n}: movimiento del periodo con su referencia y su importe en pesos.")
    }

    fn texto(tema: &str, n: usize) -> String {
        (0..n).map(|i| p(tema, i)).collect::<Vec<_>>().join("\n\n")
    }

    fn e(s: &str) -> EntityId {
        EntityId(s.to_string())
    }

    fn uno(a: &str, b: &str) -> Option<Dentro> {
        comparar(&doc(e("A"), a), &doc(e("B"), b))
    }

    #[test]
    fn un_paquete_contiene_lo_que_junta() {
        let marzo = texto("marzo", 6);
        let abril = texto("abril", 8);
        let portada = texto("portada del expediente", 2);
        let paquete = format!("{portada}\n\n{marzo}\n\n{abril}");

        let d = uno(&paquete, &marzo).expect("marzo está dentro");
        assert_eq!((d.parrafos, d.de), (6, 6));
        assert_eq!(d.confianza, 1.0);
        // El tramo es el de marzo, en el texto del paquete.
        assert_eq!(&paquete[d.bytes.0..d.bytes.1], marzo.as_str());
        assert!(uno(&paquete, &abril).is_some());
    }

    /// El caso que hunde a los fragmentos: en el paquete, el primer párrafo de
    /// abril va pegado al último de marzo (el PDF no dejó línea en blanco).
    /// Lo que no casa está en la orilla, así que sigue dentro.
    #[test]
    fn una_orilla_pegada_al_vecino_no_lo_saca() {
        let marzo = texto("marzo", 6);
        let abril = texto("abril", 30);
        let paquete = format!("{marzo}\n{abril}");
        let d = uno(&paquete, &abril).expect("29 de 30 de un tirón");
        assert_eq!((d.parrafos, d.de), (29, 30));
        assert!(d.confianza >= DE_UN_TIRON);
    }

    /// Una versión: el mismo documento con un párrafo cambiado **en medio**.
    /// Casi todo está, pero no de un tirón.
    #[test]
    fn una_version_con_un_cambio_en_medio_no_es_un_paquete() {
        let original = texto("contrato", 12);
        let mut partes: Vec<String> = (0..12).map(|i| p("contrato", i)).collect();
        partes[6] = "La cláusula sexta cambió en esta versión y ahora dice otra cosa.".into();
        partes.push(texto("anexo nuevo", 3));
        let version = partes.join("\n\n");
        assert!(uno(&version, &original).is_none());
    }

    /// El mismo documento con algo añadido al final: B es casi todo A.
    #[test]
    fn un_anadido_al_final_es_una_version_no_un_paquete() {
        let original = texto("presentación", 20);
        let con_mas = format!("{original}\n\n{}", p("una diapositiva más", 0));
        assert!(uno(&con_mas, &original).is_none());
    }

    /// Dos estados de cuenta comparten el texto legal, pero no sus movimientos.
    #[test]
    fn el_texto_legal_repetido_no_mete_uno_dentro_de_otro() {
        let legal = texto("aviso legal sobre la inflación", 3);
        let marzo = format!("{legal}\n\n{}", texto("marzo", 5));
        let abril = format!("{legal}\n\n{}", texto("abril", 9));
        assert!(uno(&abril, &marzo).is_none());
    }

    /// El título corto de un estado de cuenta no cuenta para decidir, pero se
    /// marca con él: sin esto el «¿por qué?» empezaba en la segunda línea.
    #[test]
    fn los_parrafos_cortos_de_la_orilla_se_marcan_con_el() {
        let marzo = format!(
            "MOVIMIENTOS DE MARZO\n\n{}\n\nFin de marzo.",
            texto("marzo", 5)
        );
        let paquete = format!(
            "{}\n\n{marzo}\n\nPágina 2\n\n{}",
            texto("portada", 3),
            texto("abril", 6)
        );
        let d = uno(&paquete, &marzo).expect("marzo está dentro");
        assert_eq!(&paquete[d.bytes.0..d.bytes.1], marzo.as_str());
        assert_eq!((d.parrafos, d.de), (5, 5), "los cortos no cuentan");

        // Si el de al lado no es el mismo título, no se marca de más.
        let otro = format!(
            "{}\n\nMOVIMIENTOS DE ABRIL\n\n{}",
            texto("portada", 3),
            texto("marzo", 5)
        );
        let d = uno(&otro, &marzo).expect("sin título ni cierre, sigue dentro");
        assert!(!otro[d.bytes.0..d.bytes.1].contains("ABRIL"));
    }

    #[test]
    fn un_documento_de_dos_lineas_no_cabe_en_cualquier_sitio() {
        let corto = texto("aviso", 2);
        let largo = format!("{corto}\n\n{}", texto("otra cosa", 20));
        assert!(uno(&largo, &corto).is_none());
    }

    #[test]
    fn un_paquete_mas_corto_que_la_parte_no_la_contiene() {
        let a = texto("x", 5);
        assert!(uno(&a, &a).is_none());
    }

    #[test]
    fn los_parrafos_cortos_no_cuentan_ni_rompen_el_tiron() {
        // En el paquete, cada página lleva su «Página N de 20».
        let marzo = texto("marzo", 6);
        let intercalado: Vec<String> = (0..6)
            .map(|i| format!("{}\n\nPágina {} de 20", p("marzo", i), i + 3))
            .collect();
        let paquete = format!("{}\n\n{}", texto("portada", 3), intercalado.join("\n\n"));
        let d = uno(&paquete, &marzo).expect("las páginas no cuentan");
        assert_eq!((d.parrafos, d.de), (6, 6));
    }

    #[test]
    fn la_evidencia_dice_donde_y_se_vuelve_a_leer() {
        let d = Dentro {
            parte: e("B"),
            paquete: e("A"),
            parrafos: 89,
            de: 89,
            bytes: (1234, 15678),
            confianza: 1.0,
        };
        assert_eq!(d.evidencia(), "parrafos=89/89 bytes=1234..15678");
        assert_eq!(tramo(&d.evidencia()), Some((1234, 15678)));
        assert_eq!(tramo("parrafos=1/1"), None);
    }

    // ── con la base ──────────────────────────────────────────────────────

    fn archivo(s: &Store, ruta: &str, texto: &str, mtime: i64) -> EntityId {
        let e = s.create_entity(T).unwrap();
        for (pred, v) in [
            ("path_observed_at", Value::Text(ruta.into())),
            ("modified_at", Value::Timestamp(mtime)),
        ] {
            s.assert(NewAssertion::new(
                e.clone(),
                pred,
                v,
                Method::Deterministic,
                T,
            ))
            .unwrap();
        }
        escribir(s, &e, texto);
        e
    }

    fn escribir(s: &Store, e: &EntityId, texto: &str) {
        use obrain_extract::chunk;
        obrain_extract::index::migrate(s.conn()).unwrap();
        let norma = obrain_extract::text::normalize(texto);
        obrain_extract::index::replace_chunks(
            s.conn(),
            e,
            chunk::CHUNKER,
            chunk::CHUNKER_VERSION,
            &chunk::chunk(&norma),
        )
        .unwrap();
    }

    fn vivas(s: &Store, at: i64) -> Vec<(EntityId, EntityId, String)> {
        s.query(None, Some(PREDICATE), TimePoint::at(at))
            .unwrap()
            .into_iter()
            .map(|a| {
                (
                    a.subject,
                    a.object.as_entity().unwrap().clone(),
                    a.evidence.unwrap_or_default(),
                )
            })
            .collect()
    }

    #[test]
    fn la_pasada_escribe_y_es_idempotente() {
        let s = Store::open_in_memory().unwrap();
        let marzo = texto("marzo", 6);
        let abril = texto("abril", 7);
        let b1 = archivo(&s, "C:\\v\\marzo.pdf", &marzo, 10);
        let b2 = archivo(&s, "C:\\v\\abril.pdf", &abril, 11);
        let a = archivo(&s, "C:\\v\\visa.pdf", &format!("{marzo}\n\n{abril}"), 12);

        let mut r = DentroReport::default();
        link(&s, T + 1, &mut r).unwrap();
        assert_eq!((r.documentos, r.encontrados, r.escritas), (3, 2, 2));
        let v = vivas(&s, T + 2);
        assert!(v.iter().any(|(x, y, _)| *x == b1 && *y == a));
        assert!(v.iter().any(|(x, y, _)| *x == b2 && *y == a));
        let una = s
            .query(Some(&b1), Some(PREDICATE), TimePoint::at(T + 2))
            .unwrap()
            .remove(0);
        assert_eq!(una.method, Method::Deterministic);
        assert_eq!(una.source, SOURCE);

        let mut r2 = DentroReport::default();
        link(&s, T + 3, &mut r2).unwrap();
        assert_eq!((r2.escritas, r2.retractadas, r2.sin_cambio), (0, 0, 2));
    }

    #[test]
    fn si_el_texto_cambia_la_arista_se_va() {
        let s = Store::open_in_memory().unwrap();
        let marzo = texto("marzo", 6);
        let b = archivo(&s, "C:\\v\\marzo.pdf", &marzo, 10);
        let a = archivo(
            &s,
            "C:\\v\\visa.pdf",
            &format!("{marzo}\n\n{}", texto("abril", 6)),
            11,
        );
        link(&s, T + 1, &mut DentroReport::default()).unwrap();
        assert_eq!(vivas(&s, T + 2).len(), 1);

        // Rehicieron el paquete sin marzo.
        escribir(&s, &a, &texto("abril", 6));
        let mut r = DentroReport::default();
        link(&s, T + 3, &mut r).unwrap();
        assert_eq!(r.retractadas, 1);
        assert!(vivas(&s, T + 4).is_empty());
        let _ = b;
    }

    #[test]
    fn si_el_tramo_se_mueve_se_vuelve_a_decir() {
        let s = Store::open_in_memory().unwrap();
        let marzo = texto("marzo", 6);
        archivo(&s, "C:\\v\\marzo.pdf", &marzo, 10);
        let a = archivo(
            &s,
            "C:\\v\\visa.pdf",
            &format!("{marzo}\n\n{}", texto("abril", 6)),
            11,
        );
        link(&s, T + 1, &mut DentroReport::default()).unwrap();
        let antes = vivas(&s, T + 2)[0].2.clone();

        escribir(
            &s,
            &a,
            &format!(
                "{}\n\n{marzo}\n\n{}",
                texto("portada", 3),
                texto("abril", 6)
            ),
        );
        let mut r = DentroReport::default();
        link(&s, T + 3, &mut r).unwrap();
        assert_eq!((r.retractadas, r.escritas), (1, 1));
        let v = vivas(&s, T + 4);
        assert_eq!(v.len(), 1);
        assert_ne!(v[0].2, antes, "el tramo nuevo");
    }

    /// Dos copias de marzo dentro del mismo paquete: se dice una vez, la del
    /// original (el más antiguo).
    #[test]
    fn de_los_textos_identicos_cuenta_el_original() {
        let s = Store::open_in_memory().unwrap();
        let marzo = texto("marzo", 6);
        let copia = archivo(&s, "C:\\v\\copia\\marzo.pdf", &marzo, 50);
        let original = archivo(&s, "C:\\v\\marzo.pdf", &marzo, 10);
        let a = archivo(
            &s,
            "C:\\v\\visa.pdf",
            &format!("{marzo}\n\n{}", texto("abril", 6)),
            60,
        );
        link(&s, T + 1, &mut DentroReport::default()).unwrap();
        let v = vivas(&s, T + 2);
        assert_eq!(v.len(), 1);
        assert_eq!((&v[0].0, &v[0].1), (&original, &a));
        let _ = copia;
    }

    /// Lo que está en la papelera no se toca: vuelve con lo que sabía.
    /// Lo olvidado, sí: ya no hay texto que lo sostenga.
    #[test]
    fn la_papelera_guarda_la_arista_y_el_olvido_la_quita() {
        let s = Store::open_in_memory().unwrap();
        let marzo = texto("marzo", 6);
        let b = archivo(&s, "C:\\v\\marzo.pdf", &marzo, 10);
        archivo(
            &s,
            "C:\\v\\visa.pdf",
            &format!("{marzo}\n\n{}", texto("abril", 6)),
            11,
        );
        link(&s, T + 1, &mut DentroReport::default()).unwrap();

        let marca = |pred: &str, at: i64| {
            s.assert(NewAssertion::new(
                b.clone(),
                pred,
                Value::Timestamp(at),
                Method::Deterministic,
                at,
            ))
            .unwrap();
        };
        marca(obrain_scan::ausentes::MISSING, T + 2);
        let mut r = DentroReport::default();
        link(&s, T + 3, &mut r).unwrap();
        assert_eq!((r.retractadas, r.escritas), (0, 0));
        assert_eq!(vivas(&s, T + 4).len(), 1, "en la papelera, sigue");

        marca(obrain_scan::ausentes::FORGOTTEN, T + 5);
        s.conn()
            .execute("DELETE FROM chunks WHERE entity = ?1", [b.as_str()])
            .unwrap();
        let mut r = DentroReport::default();
        link(&s, T + 6, &mut r).unwrap();
        assert_eq!(r.retractadas, 1);
        assert!(vivas(&s, T + 7).is_empty());
    }
}
