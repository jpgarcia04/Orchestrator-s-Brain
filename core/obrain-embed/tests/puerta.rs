//! El invariante I11 aplicado **a este pipeline**, no a la puerta.
//!
//! `obrain-guard` ya tiene `embeddable_excluye_la_boveda`, que prueba que la
//! consulta de origen deja fuera la bóveda. Eso demuestra que la puerta
//! funciona; no demuestra que M4 la use. Un `SELECT DISTINCT entity FROM
//! chunks` en `embed_all` pasaría aquel test y filtraría igual.
//!
//! Estos tests atacan [`obrain_embed::embed_all`]: le dan una base con una
//! entidad en bóveda que tiene fragmentos y comprueban que **no acaba con un
//! solo vector**. Es la diferencia entre cumplir I11 y decir que se cumple.
//!
//! El embebedor es de juguete a propósito: lo que se prueba es el control de
//! egreso, y hacerlo depender de que haya 450 MB de pesos en disco sería
//! convertir un test de seguridad en un test que a veces no se ejecuta.

use obrain_core::rusqlite::params;
use obrain_core::{EntityId, Method, NewAssertion, Sensitivity, Store, TimePoint};
use obrain_embed::embedder::{Embedder, ModelId, Role};
use obrain_embed::{EmbedReport, index};
use std::cell::RefCell;

const M: ModelId = ModelId {
    id: "juguete",
    version: 1,
    dim: 4,
};

/// Determinista y trazable: registra **todo** lo que se le pide embeber, que
/// es lo que permite afirmar que el texto de la bóveda no llegó al modelo.
/// Un vector escrito es una filtración; un texto *ofrecido* al modelo ya lo es.
#[derive(Default)]
struct Juguete {
    visto: RefCell<Vec<String>>,
}

impl Embedder for Juguete {
    fn model(&self) -> ModelId {
        M
    }
    fn embed(&self, texts: &[&str], _role: Role) -> obrain_embed::Result<Vec<Vec<f32>>> {
        let mut out = Vec::new();
        for t in texts {
            self.visto.borrow_mut().push(t.to_string());
            let mut v = vec![0f32; M.dim];
            for (i, b) in t.bytes().enumerate() {
                v[i % M.dim] += b as f32;
            }
            obrain_embed::embedder::normalize(&mut v);
            out.push(v);
        }
        Ok(out)
    }
    fn batch_size(&self) -> usize {
        2
    }
}

fn base() -> Store {
    let store = Store::open_in_memory().unwrap();
    obrain_extract::index::migrate(store.conn()).unwrap();
    index::migrate(store.conn()).unwrap();
    store
}

fn con_fragmentos(store: &Store, at: i64, textos: &[&str]) -> EntityId {
    let e = store.create_entity(at).unwrap();
    for (i, t) in textos.iter().enumerate() {
        store
            .conn()
            .execute(
                "INSERT INTO chunks (entity, chunker, chunker_ver, ordinal, byte_start, byte_end, text)
                 VALUES (?1, 'paragraph-pack', 1, ?2, 0, ?3, ?4)",
                params![e.as_str(), i as i64, t.len() as i64, t],
            )
            .unwrap();
    }
    e
}

fn a_la_boveda(store: &Store, e: &EntityId, at: i64) {
    store
        .assert(
            NewAssertion::new(
                e.clone(),
                "sensitivity_level",
                "vault",
                Method::Inferred,
                at,
            )
            .source("test"),
        )
        .unwrap();
    store.reproject_sensitivity(e, TimePoint::at(at)).unwrap();
    assert_eq!(store.sensitivity(e).unwrap(), Sensitivity::Vault);
}

fn vectores_de(store: &Store, e: &EntityId) -> i64 {
    store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM vectors WHERE entity = ?1",
            [e.as_str()],
            |r| r.get(0),
        )
        .unwrap()
}

/// **El test del hito.** Una entidad `vault` con fragmentos no puede acabar
/// con un solo vector.
#[test]
fn la_boveda_no_acaba_con_un_solo_vector() {
    let store = base();
    let at = 1_000;
    let normal = con_fragmentos(&store, at, &["una nomina de marzo", "y su segunda pagina"]);
    let secreto = con_fragmentos(&store, at, &["la contrasena del servidor es", "hunter2"]);
    a_la_boveda(&store, &secreto, at);

    let modelo = Juguete::default();
    let mut r = EmbedReport::default();
    obrain_embed::embed_all(&store, &modelo, at, &mut r).unwrap();

    assert_eq!(
        vectores_de(&store, &secreto),
        0,
        "la boveda ha entrado en el indice vectorial"
    );
    assert_eq!(vectores_de(&store, &normal), 2);
    assert_eq!(r.vectors_written, 2);
    assert_eq!(r.chunks_withheld, 2);
    assert_eq!(r.entities_withheld, 1);
    assert_eq!(
        r.denied_at_call, 0,
        "embeddable y check tienen que estar de acuerdo"
    );

    // Y el texto de la bóveda **ni siquiera se le ofreció al modelo**. Con un
    // modelo local la diferencia parece académica; con uno remoto es la
    // diferencia entre un secreto que sale del equipo y uno que no.
    let visto = modelo.visto.borrow();
    assert!(
        !visto.iter().any(|t| t.contains("hunter2")),
        "el texto de la boveda llego al modelo: {visto:?}"
    );
}

/// Y tampoco al log: si M4 escribiera `embedded_with` para una entidad de
/// bóveda, el log afirmaría algo que no ocurrió.
#[test]
fn la_boveda_tampoco_deja_procedencia() {
    let store = base();
    let at = 1_000;
    let secreto = con_fragmentos(&store, at, &["clave"]);
    a_la_boveda(&store, &secreto, at);

    let mut r = EmbedReport::default();
    obrain_embed::embed_all(&store, &Juguete::default(), at, &mut r).unwrap();

    let tp = TimePoint::at(at);
    assert!(
        store
            .resolve(&secreto, "embedded_with", tp)
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .resolve(&secreto, "vector_count", tp)
            .unwrap()
            .is_none()
    );
}

/// Clasificar algo como bóveda **después** de vectorizarlo es el caso
/// desagradable: los vectores ya existen. Que la clasificación no los borre es
/// correcto —I1 dice que el índice es una proyección y quien lo reconstruye es
/// quien lo mantiene—, pero entonces la reconstrucción **tiene** que dejarlo
/// fuera, o la bóveda sería una etiqueta sin efecto.
#[test]
fn reclasificar_a_boveda_saca_del_indice_al_reconstruir() {
    let store = base();
    let at = 1_000;
    let e = con_fragmentos(&store, at, &["algo que luego resulta ser secreto"]);

    let mut r = EmbedReport::default();
    obrain_embed::embed_all(&store, &Juguete::default(), at, &mut r).unwrap();
    assert_eq!(vectores_de(&store, &e), 1);

    a_la_boveda(&store, &e, at + 1);

    // Reconstruir la proyección es la operación normal de I1.
    store.conn().execute("DELETE FROM vectors", []).unwrap();
    let mut r2 = EmbedReport::default();
    obrain_embed::embed_all(&store, &Juguete::default(), at + 2, &mut r2).unwrap();

    assert_eq!(vectores_de(&store, &e), 0);
    assert_eq!(r2.vectors_written, 0);
}

/// Y sin reconstruir nada: la pasada siguiente de vectorización ya lo saca.
/// Es el caso de una foto que se mete en una carpeta marcada como bóveda
/// (ADR-0020): la marca no conoce la tabla de vectores, pero quien la mantiene
/// sí conoce la puerta.
#[test]
fn la_pasada_siguiente_saca_del_indice_lo_que_subio_a_boveda() {
    let store = base();
    let at = 1_000;
    let e = con_fragmentos(&store, at, &["un escaneo que luego va a la boveda"]);
    let otro = con_fragmentos(&store, at, &["un recibo normal"]);

    let mut r = EmbedReport::default();
    obrain_embed::embed_all(&store, &Juguete::default(), at, &mut r).unwrap();
    assert_eq!(vectores_de(&store, &e), 1);
    assert!(
        store
            .resolve(&e, "embedded_with", TimePoint::at(at))
            .unwrap()
            .is_some()
    );

    a_la_boveda(&store, &e, at + 1);
    let mut r2 = EmbedReport::default();
    obrain_embed::embed_all(&store, &Juguete::default(), at + 2, &mut r2).unwrap();

    assert_eq!(vectores_de(&store, &e), 0);
    assert_eq!(vectores_de(&store, &otro), 1, "lo demás no se toca");
    assert_eq!(r2.entities_purged, 1);
    let tp = TimePoint::at(at + 2);
    assert!(store.resolve(&e, "embedded_with", tp).unwrap().is_none());
    assert!(store.resolve(&e, "vector_count", tp).unwrap().is_none());

    // Y otra pasada ya no tiene nada que sacar.
    let mut r3 = EmbedReport::default();
    obrain_embed::embed_all(&store, &Juguete::default(), at + 3, &mut r3).unwrap();
    assert_eq!(r3.entities_purged, 0);
}

/// Lo `sensitive` **sí** entra en el índice: §5.2 le concede índice vectorial
/// local. Un control que también dejara fuera lo sensible sería un control que
/// nadie mantiene encendido, que es el modo real en que fallan.
#[test]
fn lo_sensible_si_se_vectoriza_en_local() {
    let store = base();
    let at = 1_000;
    let e = con_fragmentos(&store, at, &["matricula con un NIE dentro"]);
    store
        .assert(
            NewAssertion::new(
                e.clone(),
                "sensitivity_level",
                "sensitive",
                Method::Inferred,
                at,
            )
            .source("test"),
        )
        .unwrap();
    store.reproject_sensitivity(&e, TimePoint::at(at)).unwrap();

    let mut r = EmbedReport::default();
    obrain_embed::embed_all(&store, &Juguete::default(), at, &mut r).unwrap();
    assert_eq!(vectores_de(&store, &e), 1);
}

/// Idempotencia: volver a pasar no reembebe nada ni infla el log.
#[test]
fn una_segunda_pasada_no_hace_nada() {
    let store = base();
    let at = 1_000;
    con_fragmentos(&store, at, &["uno", "dos", "tres"]);

    let mut r = EmbedReport::default();
    obrain_embed::embed_all(&store, &Juguete::default(), at, &mut r).unwrap();
    assert_eq!(r.vectors_written, 3);
    assert_eq!(r.assertions_written, 2);

    let mut r2 = EmbedReport::default();
    obrain_embed::embed_all(&store, &Juguete::default(), at + 1, &mut r2).unwrap();
    assert_eq!(r2.pending, 0);
    assert_eq!(r2.vectors_written, 0);
    assert_eq!(r2.assertions_written, 0);
    assert_eq!(r2.assertions_unchanged, 2);
}
