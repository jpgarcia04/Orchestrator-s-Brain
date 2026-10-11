use obrain_api::Brain;
use obrain_core::{EntityId, Method, NewAssertion, Sensitivity, Store, Value};

fn fixture() -> (Brain, EntityId) {
    let s = Store::open_in_memory().unwrap();
    let e = s.create_entity(1).unwrap();
    s.assert(NewAssertion::new(
        e.clone(),
        "path_observed_at",
        "fixtures/documento.txt",
        Method::Deterministic,
        1,
    ))
    .unwrap();
    (Brain::from_store(s), e)
}

#[test]
fn rechaza_observaciones_y_controles_sin_escribir() {
    let (b, e) = fixture();
    let n = b.store().log_len().unwrap();
    for p in [
        "path_observed_at",
        "folder_path",
        "file_id",
        "availability",
        "has_hash",
        "has_facet",
        "contents_sensitivity",
        "member_of",
        "downloaded_from",
        "extraction_status",
    ] {
        assert!(
            b.correct(&e, p, Value::Text("fabricado".into())).is_err(),
            "aceptó {p}"
        );
    }
    assert_eq!(b.store().log_len().unwrap(), n);
    assert_eq!(
        b.entity(&e).unwrap().path.as_deref(),
        Some("fixtures/documento.txt")
    );
}

#[test]
fn valida_tipos_y_fechas() {
    let (b, e) = fixture();
    for (p, v) in [
        ("version_of", Value::Text(e.0.clone())),
        ("expires_on", Value::Bool(true)),
        ("expires_on", Value::Text("2030-02-30".into())),
        ("sensitivity_level", Value::Text("publico".into())),
    ] {
        assert!(b.correct(&e, p, v).is_err(), "aceptó tipo/valor para {p}");
    }
}

#[test]
fn correccion_generica_no_salta_el_pin() {
    let (b, e) = fixture();
    b.store()
        .set_sensitivity(&e, Sensitivity::Sensitive, 1)
        .unwrap();
    b.set_pin("2468", None).unwrap();
    b.lock();
    assert!(b.set_expiry(&e, Some("2030-01-01")).is_err());
    assert!(
        b.correct(&e, "expires_on", Value::Text("2030-01-01".into()))
            .is_err(),
        "saltó el PIN"
    );
    assert!(
        b.correct(&e, "sensitivity_level", Value::Text("normal".into()))
            .is_err()
    );
    b.unlock("2468").unwrap();
    b.correct(&e, "expires_on", Value::Text("2030-01-01".into()))
        .unwrap();
}

#[test]
fn una_url_y_evidencia_protegidas_no_se_serializan() {
    let (b, e) = fixture();
    b.store()
        .assert(
            NewAssertion::new(
                e.clone(),
                "downloaded_from",
                "https://portal.example/documento?token=SINTETICO",
                Method::Deterministic,
                1,
            )
            .evidence("literal PRIVADO")
            .source("source PRIVADO"),
        )
        .unwrap();
    b.store()
        .set_sensitivity(&e, Sensitivity::Sensitive, 1)
        .unwrap();
    b.set_pin("2468", None).unwrap();
    b.lock();
    let json = serde_json::to_string(&b.entity(&e).unwrap()).unwrap();
    assert!(
        !json.contains("SINTETICO") && !json.contains("PRIVADO"),
        "salieron metadatos protegidos"
    );
    assert!(
        json.contains("fixtures/documento.txt"),
        "la ruta es pública por diseño"
    );
}

#[test]
fn corregir_sensibilidad_actualiza_la_puerta() {
    let (b, e) = fixture();
    b.correct(&e, "sensitivity_level", Value::Text("sensitive".into()))
        .unwrap();
    assert_eq!(b.store().sensitivity(&e).unwrap(), Sensitivity::Sensitive);
    b.set_pin("2468", None).unwrap();
    b.lock();
    assert!(!b.may_open(&e).unwrap());
}

#[test]
fn dos_decisiones_de_sensibilidad_en_el_mismo_instante_son_la_ultima() {
    let (b, e) = fixture();
    for level in ["sensitive", "normal", "vault"] {
        b.store()
            .assert_by_user(NewAssertion::new(
                e.clone(),
                "sensitivity_level",
                level,
                Method::User,
                100,
            ))
            .unwrap();
        b.store()
            .reproject_sensitivity(&e, obrain_core::TimePoint::at(100))
            .unwrap();
        assert_eq!(b.store().sensitivity(&e).unwrap().as_str(), level);
    }
    let claims = b
        .store()
        .query(
            Some(&e),
            Some("sensitivity_level"),
            obrain_core::TimePoint::at(100),
        )
        .unwrap();
    assert_eq!(claims.len(), 1);
    assert_eq!(
        b.store().log_len().unwrap(),
        4,
        "las decisiones anteriores no se borran"
    );
}

#[test]
fn confirma_una_relacion_con_su_objeto_y_rechaza_ids_ajenos() {
    let (b, e) = fixture();
    let target = b.store().create_entity(1).unwrap();
    let claim = b
        .store()
        .assert(NewAssertion::new(
            e.clone(),
            "version_of",
            Value::Entity(target.clone()),
            Method::Inferred,
            1,
        ))
        .unwrap();
    let id = b.confirm(&e, claim.as_str()).unwrap();
    assert_eq!(id, b.confirm(&e, claim.as_str()).unwrap());
    let v = b.entity(&e).unwrap();
    let c = v.observed.iter().find(|c| c.assertion == id).unwrap();
    assert_eq!(c.value_entity.as_deref(), Some(target.as_str()));
    assert!(c.by_user);
    assert!(b.confirm(&target, claim.as_str()).is_err());
    b.store().retract(&claim, 2).unwrap();
    assert!(b.confirm(&e, claim.as_str()).is_err());
}

#[test]
fn ni_la_confirmacion_ni_el_historial_fabrican_rutas_de_apertura() {
    let (b, e) = fixture();
    let injected = b
        .store()
        .assert(NewAssertion::new(
            e.clone(),
            "path_observed_at",
            "fixtures/ajeno.txt",
            Method::User,
            2,
        ))
        .unwrap();
    assert!(b.confirm(&e, injected.as_str()).is_err());
    assert_eq!(
        b.path_to_open(&e).unwrap().as_deref(),
        Some("fixtures/documento.txt")
    );
    assert_eq!(
        b.entity(&e).unwrap().path.as_deref(),
        Some("fixtures/documento.txt")
    );
    b.store()
        .set_sensitivity(&e, Sensitivity::Sensitive, 1)
        .unwrap();
    b.set_pin("2468", None).unwrap();
    b.lock();
    assert!(matches!(
        b.path_to_open(&e),
        Err(obrain_api::Error::Bloqueado)
    ));
}

#[test]
fn las_escrituras_alternativas_comprueban_el_pin_antes_de_escribir() {
    let (mut b, e) = fixture();
    let collection = EntityId(b.create_collection("Fixture", "test").unwrap());
    let second = EntityId(b.create_collection("Otra", "test").unwrap());
    b.add_to_collection(&collection, std::slice::from_ref(&e), "test")
        .unwrap();
    let folder = b.store().create_entity(1).unwrap();
    b.store()
        .assert(NewAssertion::new(
            folder.clone(),
            "folder_path",
            "fixtures",
            Method::Deterministic,
            1,
        ))
        .unwrap();
    b.store()
        .set_sensitivity(&e, Sensitivity::Sensitive, 1)
        .unwrap();
    b.set_pin("2468", None).unwrap();
    b.lock();
    let n = b.store().log_len().unwrap();
    assert!(matches!(
        b.mark_folder(&folder, None),
        Err(obrain_api::Error::Bloqueado)
    ));
    assert!(matches!(
        b.add_to_collection(&second, std::slice::from_ref(&e), "test"),
        Err(obrain_api::Error::Bloqueado)
    ));
    assert!(matches!(
        b.remove_from_collection(&collection, std::slice::from_ref(&e)),
        Err(obrain_api::Error::Bloqueado)
    ));
    assert!(matches!(
        b.move_between_collections(&collection, &second, std::slice::from_ref(&e), "test"),
        Err(obrain_api::Error::Bloqueado)
    ));
    assert!(matches!(
        b.delete_collection(&collection),
        Err(obrain_api::Error::Bloqueado)
    ));
    assert!(matches!(
        b.forget(Some(&e)),
        Err(obrain_api::Error::Bloqueado)
    ));
    assert_eq!(b.store().log_len().unwrap(), n);
    b.unlock("2468").unwrap();
    b.move_between_collections(&collection, &second, &[e], "test")
        .unwrap();
}

#[test]
fn la_confirmacion_y_la_relacion_no_saltan_proteccion_del_destino() {
    let (b, e) = fixture();
    let target = b.store().create_entity(1).unwrap();
    b.store()
        .set_sensitivity(&target, Sensitivity::Sensitive, 1)
        .unwrap();
    let claim = b
        .store()
        .assert(NewAssertion::new(
            e.clone(),
            "version_of",
            Value::Entity(target.clone()),
            Method::Inferred,
            1,
        ))
        .unwrap();
    b.set_pin("2468", None).unwrap();
    b.lock();
    assert!(b.confirm(&e, claim.as_str()).is_err());
    assert!(b.correct(&e, "version_of", Value::Entity(target)).is_err());
    b.unlock("2468").unwrap();
    b.confirm(&e, claim.as_str()).unwrap();
}

#[test]
fn un_error_de_seguridad_no_es_ausencia_de_pin() {
    let (b, e) = fixture();
    b.store()
        .set_sensitivity(&e, Sensitivity::Sensitive, 1)
        .unwrap();
    b.store()
        .conn()
        .execute_batch("DROP TABLE seguridad")
        .unwrap();
    assert!(b.security().is_err());
    assert!(b.preview(&e, None).is_err());
    assert!(b.entity(&e).is_err());
    assert!(b.search("documento", 10).is_err());
    assert!(b.may_open(&e).is_err());
    assert!(b.correct(&e, "sensitivity_level", "normal".into()).is_err());
}

#[test]
fn rollback_conserva_respuesta_anterior_y_proyeccion() {
    let (b, e) = fixture();
    b.correct(&e, "expires_on", "2030-01-01".into()).unwrap();
    b.store().conn().execute_batch("CREATE TRIGGER fallo_sintetico BEFORE INSERT ON assertions WHEN NEW.method = 'user' BEGIN SELECT RAISE(ABORT, 'fixture'); END;").unwrap();
    assert!(b.set_expiry(&e, Some("2031-01-01")).is_err());
    let c = b
        .entity(&e)
        .unwrap()
        .observed
        .into_iter()
        .find(|c| c.predicate == "expires_on")
        .unwrap();
    assert_eq!(c.value, "2030-01-01");
    b.store().conn().execute_batch("DROP TRIGGER fallo_sintetico; CREATE TRIGGER fallo_proyeccion BEFORE UPDATE OF sensitivity ON entities BEGIN SELECT RAISE(ABORT, 'fixture'); END;").unwrap();
    let n = b.store().log_len().unwrap();
    assert!(
        b.correct(&e, "sensitivity_level", "sensitive".into())
            .is_err()
    );
    assert_eq!(b.store().log_len().unwrap(), n);
    assert_eq!(b.store().sensitivity(&e).unwrap(), Sensitivity::Normal);
}

#[test]
fn la_sensibilidad_del_paquete_se_actualiza_y_los_vectores_se_revocan_sin_modelo() {
    let (b, e) = fixture();
    let package = b.store().create_entity(1).unwrap();
    b.store()
        .assert(NewAssertion::new(
            e.clone(),
            "part_of",
            Value::Entity(package.clone()),
            Method::Deterministic,
            1,
        ))
        .unwrap();
    let chunk = |entity: &EntityId| {
        b.store().conn().execute("INSERT INTO chunks (entity, chunker, chunker_ver, ordinal, byte_start, byte_end, text) VALUES (?1, 'fixture', 1, 0, 0, 9, 'SINTETICO')", [entity.as_str()]).unwrap();
        let id = b.store().conn().last_insert_rowid();
        obrain_embed::index::insert(
            b.store().conn(),
            &obrain_embed::index::Pending {
                chunk: id,
                entity: entity.clone(),
                chunker: "fixture".into(),
                chunker_ver: 1,
                text: "SINTETICO".into(),
            },
            obrain_embed::embedder::ModelId {
                id: "fixture",
                version: 1,
                dim: 2,
            },
            &[1.0, 0.0],
        )
        .unwrap();
    };
    chunk(&e);
    chunk(&package);
    for entity in [&e, &package] {
        for (predicate, value) in [
            ("embedded_with", Value::Text("fixture@1".into())),
            ("vector_count", Value::Number(1.0)),
        ] {
            b.store()
                .assert(NewAssertion::new(
                    entity.clone(),
                    predicate,
                    value,
                    Method::Deterministic,
                    1,
                ))
                .unwrap();
        }
    }
    b.correct(&e, "sensitivity_level", "vault".into()).unwrap();
    let vectors: i64 = b
        .store()
        .conn()
        .query_row("SELECT count(*) FROM vectors", [], |r| r.get(0))
        .unwrap();
    assert_eq!(vectors, 0);
    for entity in [&e, &package] {
        for predicate in ["embedded_with", "vector_count"] {
            assert!(
                b.store()
                    .resolve(
                        entity,
                        predicate,
                        obrain_core::TimePoint::at(obrain_scan::now_ms())
                    )
                    .unwrap()
                    .is_none(),
                "procedencia vigente: {predicate}"
            );
            assert!(
                b.store()
                    .resolve(entity, predicate, obrain_core::TimePoint::at(1))
                    .unwrap()
                    .is_some()
            );
        }
    }
    assert_eq!(b.store().sensitivity(&package).unwrap(), Sensitivity::Vault);
    b.set_pin("2468", None).unwrap();
    b.lock();
    assert!(!b.may_open(&package).unwrap());
    b.unlock("2468").unwrap();
    b.correct(&e, "sensitivity_level", "normal".into()).unwrap();
    assert_eq!(
        b.store().sensitivity(&package).unwrap(),
        Sensitivity::Normal
    );
}
