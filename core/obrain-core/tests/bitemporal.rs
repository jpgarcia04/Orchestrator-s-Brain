//! Criterio de "hecho" de M0 (`PROJECT-MASTER.md` §9):
//! *se pueden escribir, retractar y consultar aserciones bitemporales.*
//!
//! Y por encima de eso, la verificación de los invariantes que no pueden
//! romperse nunca: I1, I4, I5 e I6.

use obrain_core::*;

// Línea temporal legible (epoch ms ficticios, escala de "años").
const T_2021: i64 = 1_600_000_000_000;
const T_2022: i64 = 1_640_000_000_000;
const T_2023_MAR: i64 = 1_678_000_000_000;
const T_2023_NOV: i64 = 1_699_000_000_000;
const T_2024_FEB: i64 = 1_707_000_000_000;
const T_HOY: i64 = 1_750_000_000_000;

fn store() -> Store {
    Store::open_in_memory().expect("abrir store en memoria")
}

// ═══════════════════════════════════════════════════ básico: escribir y leer

#[test]
fn escribe_y_lee_una_asercion() {
    let s = store();
    let doc = s.create_entity(T_2021).unwrap();

    s.assert(
        NewAssertion::new(doc.clone(), "about", "impuestos", Method::Inferred, T_2022)
            .confidence(0.7)
            .source("classifier@v1")
            .evidence("pagina 1, offset 240-268"),
    )
    .unwrap();

    let found = s.query(Some(&doc), Some("about"), TimePoint::at(T_HOY)).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].object.as_text(), Some("impuestos"));
    assert_eq!(found[0].method, Method::Inferred);
    assert_eq!(found[0].confidence, 0.7);
    assert_eq!(found[0].evidence.as_deref(), Some("pagina 1, offset 240-268"));
}

// ═══════════════════════════════════════════════════════ I4 · nada se borra

#[test]
fn retractar_no_borra_deja_de_estar_vigente() {
    let s = store();
    let doc = s.create_entity(T_2021).unwrap();
    let id = s
        .assert(NewAssertion::new(doc.clone(), "about", "japon", Method::Inferred, T_2022))
        .unwrap();

    assert_eq!(s.log_len().unwrap(), 1);
    s.retract(&id, T_2023_MAR).unwrap();

    // el log NO encoge
    assert_eq!(s.log_len().unwrap(), 1);

    // ya no está vigente hoy
    assert!(s.query(Some(&doc), Some("about"), TimePoint::at(T_HOY)).unwrap().is_empty());

    // pero sigue siendo visible como creencia pasada — esto es "recordar"
    let antes = s.query(Some(&doc), Some("about"), TimePoint::at(T_2022)).unwrap();
    assert_eq!(antes.len(), 1, "la creencia pasada tiene que sobrevivir a la retraccion");
}

#[test]
fn el_motor_impide_borrar_aserciones() {
    let s = store();
    let doc = s.create_entity(T_2021).unwrap();
    s.assert(NewAssertion::new(doc, "about", "x", Method::Inferred, T_2022)).unwrap();

    let err = s.conn().execute("DELETE FROM assertions", []).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("I4"), "el trigger debe rechazar el DELETE, dijo: {msg}");
    assert!(Error::from(err).is_invariant_violation());
    assert_eq!(s.log_len().unwrap(), 1);
}

#[test]
fn la_retraccion_es_definitiva() {
    let s = store();
    let doc = s.create_entity(T_2021).unwrap();
    let id = s
        .assert(NewAssertion::new(doc, "about", "x", Method::Inferred, T_2022))
        .unwrap();

    s.retract(&id, T_2023_MAR).unwrap();
    let err = s.retract(&id, T_2024_FEB).unwrap_err();
    assert!(err.to_string().contains("I4"), "no se puede reescribir la historia: {err}");
}

// ═════════════════════════════════════════════════════ I1 · log inmutable

#[test]
fn el_motor_impide_mutar_el_log() {
    let s = store();
    let doc = s.create_entity(T_2021).unwrap();
    s.assert(NewAssertion::new(doc, "about", "x", Method::Inferred, T_2022)).unwrap();

    for sql in [
        "UPDATE assertions SET confidence = 0.01",
        "UPDATE assertions SET object_text = 'otra cosa'",
        "UPDATE assertions SET method = 'user'",
        "UPDATE assertions SET asserted_at = 0",
        "UPDATE assertions SET source = 'falsificado'",
    ] {
        let err = s.conn().execute(sql, []).unwrap_err();
        assert!(err.to_string().contains("I1"), "deberia rechazar `{sql}`, dijo: {err}");
    }
}

// ══════════════════════════════════════════════════════ I6 · bitemporalidad

#[test]
fn eje_de_validez_que_era_cierto_en_el_mundo() {
    let s = store();
    let persona = s.create_entity(T_2021).unwrap();

    // Trabajó en Acme entre 2021 y 2023.
    s.assert(
        NewAssertion::new(persona.clone(), "about", "Acme", Method::User, T_2021)
            .valid(Some(T_2021), Some(T_2023_MAR)),
    )
    .unwrap();
    // Y en Globex desde 2023.
    s.assert(
        NewAssertion::new(persona.clone(), "about", "Globex", Method::User, T_2023_MAR)
            .valid(Some(T_2023_MAR), None),
    )
    .unwrap();

    let en_2022 = s.resolve(&persona, "about", TimePoint::at(T_2022)).unwrap().unwrap();
    assert_eq!(en_2022.object.as_text(), Some("Acme"));

    let hoy = s.resolve(&persona, "about", TimePoint::at(T_HOY)).unwrap().unwrap();
    assert_eq!(hoy.object.as_text(), Some("Globex"));
}

#[test]
fn eje_de_conocimiento_que_sabia_yo_en_marzo_de_2023() {
    let s = store();
    let doc = s.create_entity(T_2021).unwrap();

    // En 2022 el sistema creía que el documento trataba de impuestos.
    let viejo = s
        .assert(NewAssertion::new(doc.clone(), "about", "impuestos", Method::Inferred, T_2022))
        .unwrap();
    // En noviembre de 2023 se dio cuenta de que era sobre la visa.
    s.retract(&viejo, T_2023_NOV).unwrap();
    s.assert(NewAssertion::new(doc.clone(), "about", "visa", Method::Inferred, T_2023_NOV))
        .unwrap();

    // "¿Qué creía el sistema en marzo de 2023?"
    let creencia = s
        .resolve(&doc, "about", TimePoint::as_known_at(T_2023_MAR, T_2023_MAR))
        .unwrap()
        .unwrap();
    assert_eq!(creencia.object.as_text(), Some("impuestos"));

    // "¿Y qué cree hoy?"
    let ahora = s.resolve(&doc, "about", TimePoint::at(T_HOY)).unwrap().unwrap();
    assert_eq!(ahora.object.as_text(), Some("visa"));
}

// ══════════════════════ I5 · el usuario gana. SIEMPRE. Éste es EL test.

#[test]
fn precedencia_del_usuario_sobre_la_inferencia() {
    let s = store();
    let factura = s.create_entity(T_2021).unwrap();

    s.assert(
        NewAssertion::new(factura.clone(), "part_of_process", "viaje-japon", Method::Inferred, T_2022)
            .confidence(0.88),
    )
    .unwrap();
    s.assert(NewAssertion::new(
        factura.clone(),
        "part_of_process",
        "mudanza",
        Method::User,
        T_2023_MAR,
    ))
    .unwrap();

    let ganador = s
        .resolve(&factura, "part_of_process", TimePoint::at(T_HOY))
        .unwrap()
        .unwrap();
    assert_eq!(ganador.method, Method::User);
    assert_eq!(ganador.object.as_text(), Some("mudanza"));
}

/// **El test que decide si el producto existe.**
///
/// Si una corrección del usuario puede ser revertida por un reescaneo
/// posterior, el usuario pierde la confianza en el sistema para siempre y
/// nunca le confiará un pasaporte. Ver `PROJECT-MASTER.md` invariante I5.
#[test]
fn la_correccion_del_usuario_sobrevive_a_un_reescaneo() {
    let s = store();
    let factura = s.create_entity(T_2021).unwrap();

    // 1 · el clasificador infiere, con confianza alta
    s.assert(
        NewAssertion::new(factura.clone(), "part_of_process", "viaje-japon", Method::Inferred, T_2022)
            .confidence(0.93)
            .source("classifier@v1"),
    )
    .unwrap();

    // 2 · el usuario corrige
    s.assert(NewAssertion::new(
        factura.clone(),
        "part_of_process",
        "mudanza",
        Method::User,
        T_2023_MAR,
    ))
    .unwrap();

    // 3 · MESES DESPUÉS, un reescaneo vuelve a inferir lo de antes,
    //     con más confianza todavía y un modelo más nuevo.
    s.assert(
        NewAssertion::new(factura.clone(), "part_of_process", "viaje-japon", Method::Inferred, T_2024_FEB)
            .confidence(0.99)
            .source("classifier@v4"),
    )
    .unwrap();

    // 4 · la corrección del usuario sigue en pie.
    let ganador = s
        .resolve(&factura, "part_of_process", TimePoint::at(T_HOY))
        .unwrap()
        .unwrap();
    assert_eq!(
        ganador.method,
        Method::User,
        "un reescaneo NUNCA puede desplazar una correccion del usuario (I5)"
    );
    assert_eq!(ganador.object.as_text(), Some("mudanza"));

    // 5 · y la inferencia descartada no se ha perdido: sigue en el log,
    //     visible y explicable. No se borra, sólo pierde.
    assert_eq!(s.log_len().unwrap(), 3);
}

// ═══════════════════════════════════════════════════════ contradicciones

#[test]
fn deteccion_de_contradicciones_es_una_consulta_no_un_job() {
    let s = store();
    let doc = s.create_entity(T_2021).unwrap();

    // `contained_by` tiene cardinalidad `one`: dos valores vivos = contradicción
    s.assert(
        NewAssertion::new(doc.clone(), "contained_by", "carpeta-a", Method::Deterministic, T_2022)
            .source("scan@2022"),
    )
    .unwrap();
    s.assert(
        NewAssertion::new(doc.clone(), "contained_by", "carpeta-b", Method::Inferred, T_2023_MAR)
            .confidence(0.7),
    )
    .unwrap();

    let c = s.contradictions(TimePoint::at(T_HOY)).unwrap();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].predicate, "contained_by");
    assert_eq!(c[0].assertions.len(), 2);
    // ordenadas por precedencia: gana el determinista sobre el inferido
    assert_eq!(c[0].assertions[0].method, Method::Deterministic);
}

#[test]
fn cardinalidad_many_no_es_contradiccion() {
    let s = store();
    let doc = s.create_entity(T_2021).unwrap();
    // `mentions` es `many`: mencionar dos cosas es normal
    for t in ["irpf", "nomina"] {
        s.assert(NewAssertion::new(doc.clone(), "mentions", t, Method::Deterministic, T_2022))
            .unwrap();
    }
    assert!(s.contradictions(TimePoint::at(T_HOY)).unwrap().is_empty());
}

// ═══════════════════════════════════════════════════ facetas y sensibilidad

#[test]
fn las_facetas_son_aserciones_y_pueden_inferirse() {
    let s = store();
    let doc = s.create_entity(T_2021).unwrap();

    // deterministas, del propio archivo
    for f in [Facet::Addressable, Facet::Textual] {
        s.assert(NewAssertion::new(doc.clone(), "has_facet", f.as_str(), Method::Deterministic, T_2022))
            .unwrap();
    }
    // inferida por un clasificador — con confianza, como debe ser
    s.assert(
        NewAssertion::new(doc.clone(), "has_facet", Facet::Financial.as_str(), Method::Inferred, T_2023_MAR)
            .confidence(0.81),
    )
    .unwrap();

    let mut facetas = s.facets(&doc, TimePoint::at(T_HOY)).unwrap();
    facetas.sort();
    assert_eq!(facetas, vec!["addressable", "financial", "textual"]);
}

#[test]
fn vault_no_puede_salir_de_la_maquina() {
    let s = store();
    let pasaporte = s.create_entity_with(T_2021, Sensitivity::Vault).unwrap();
    let nota = s.create_entity(T_2021).unwrap();

    assert!(!s.sensitivity(&pasaporte).unwrap().may_egress());
    assert!(s.sensitivity(&nota).unwrap().may_egress());

    // subir el nivel es una operación de seguridad, no una aserción
    s.set_sensitivity(&nota, Sensitivity::Sensitive).unwrap();
    assert!(!s.sensitivity(&nota).unwrap().may_egress());
}

// ═════════════════════════════════════════ la pregunta que valida el proyecto

/// Ensayo, a nivel de datos, de la pregunta canónica:
/// *"¿dónde estaba el documento que usé para demostrar ingresos durante mi
/// trámite de visa?"*
///
/// M0 no tiene búsqueda ni lenguaje natural. Lo que este test demuestra es que
/// **el modelo de datos ya contiene la forma de la respuesta**: resolver el
/// proceso, leer su intervalo, y filtrar artefactos por rol dentro de él.
#[test]
fn la_pregunta_de_la_visa_a_nivel_de_datos() {
    let s = store();

    // El sistema detecta un proceso — nadie lo escribió a mano.
    let visa = s.create_entity(T_2023_NOV).unwrap();
    s.assert(
        NewAssertion::new(visa.clone(), "has_facet", Facet::Process.as_str(), Method::Inferred, T_2023_NOV)
            .confidence(0.84)
            .source("process-detector@v1"),
    )
    .unwrap();
    s.assert(
        NewAssertion::new(visa.clone(), "about", "tramite de visa", Method::Inferred, T_2023_NOV)
            .confidence(0.84)
            .valid(Some(T_2023_NOV), Some(T_2024_FEB)),
    )
    .unwrap();

    // Tres nóminas, más un documento señuelo fuera del proceso.
    let mut nominas = Vec::new();
    for mes in 0..3 {
        let n = s.create_entity(T_2023_NOV + mes).unwrap();
        s.assert(NewAssertion::new(
            n.clone(),
            "part_of_process",
            Value::Entity(visa.clone()),
            Method::Inferred,
            T_2023_NOV,
        ))
        .unwrap();
        s.assert(
            NewAssertion::new(n.clone(), "evidence_for", "ingresos", Method::Inferred, T_2023_NOV)
                .confidence(0.9),
        )
        .unwrap();
        nominas.push(n);
    }
    let recibo_luz = s.create_entity(T_2023_NOV).unwrap();
    s.assert(NewAssertion::new(
        recibo_luz,
        "evidence_for",
        "domicilio",
        Method::Inferred,
        T_2023_NOV,
    ))
    .unwrap();

    // ── la consulta ──
    let at = TimePoint::at(T_HOY);

    // 1 · artefactos del proceso
    let miembros: Vec<EntityId> = s
        .query(None, Some("part_of_process"), at)
        .unwrap()
        .into_iter()
        .filter(|a| a.object.as_entity() == Some(&visa))
        .map(|a| a.subject)
        .collect();
    assert_eq!(miembros.len(), 3);

    // 2 · de ésos, los que prueban ingresos
    let prueba_ingresos: Vec<&EntityId> = miembros
        .iter()
        .filter(|e| {
            s.resolve(e, "evidence_for", at)
                .unwrap()
                .and_then(|a| a.object.as_text().map(|t| t == "ingresos"))
                .unwrap_or(false)
        })
        .collect();

    assert_eq!(prueba_ingresos.len(), 3, "las tres nominas, y no el recibo de la luz");
}
