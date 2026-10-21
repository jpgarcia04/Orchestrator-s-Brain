//! Respaldos consistentes, verificables y ensayables de una base cifrada.
//!
//! Comprueba el código de producción (`obrain_api::respaldo` y el binario
//! `obrain`) usando exclusivamente bases ficticias y archivos temporales.

use obrain_api::respaldo::{
    self, BASE_ARCHIVO, Error as RespaldoError, EtapaRespaldo, LLAVE_ARCHIVO, MANIFIESTO_ARCHIVO,
    Manifiesto,
};
use obrain_api::seguridad::{self, abrir_con};
use obrain_api::{Brain, Embedder, Error, indexar};
use obrain_core::{EntityId, Method, NewAssertion, Sensitivity, Store, TimePoint};
use obrain_keys::KdfParams;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

const K: KdfParams = KdfParams::FAST_FOR_TESTS;
const SECRETO_BOVEDA: &str = "PASAPORTE SINTETICO Z99887766 EXPIRA 2031-05-14";
const SECRETO_SENSIBLE: &str = "NOMINA SINTETICA CUENTA ES00-0000-0000-0000";

struct CarpetaPrueba {
    ruta: PathBuf,
}

impl CarpetaPrueba {
    fn nueva(nombre: &str) -> Self {
        let mut azar = [0u8; 6];
        let _ = getrandom::fill(&mut azar);
        let mut suf = String::with_capacity(12);
        for b in azar {
            use std::fmt::Write as _;
            let _ = write!(&mut suf, "{b:02x}");
        }
        let ruta = std::env::temp_dir().join(format!(
            "obrain-respaldos-{nombre}-{}-{suf}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&ruta);
        std::fs::create_dir_all(&ruta).unwrap();
        Self { ruta }
    }

    fn path(&self) -> &Path {
        &self.ruta
    }
}

impl Drop for CarpetaPrueba {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.ruta);
    }
}

/// Modelo sintético determinista para probar también la tabla `vectors` sin
/// requerir la descarga de `multilingual-e5-small`.
struct ModeloFicticio;

impl Embedder for ModeloFicticio {
    fn model(&self) -> obrain_embed::embedder::ModelId {
        obrain_embed::embedder::ModelId {
            id: "modelo-ficticio-pruebas",
            version: 1,
            dim: 4,
        }
    }

    fn embed(
        &self,
        texts: &[&str],
        _role: obrain_embed::embedder::Role,
    ) -> obrain_embed::Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| vector_de(t)).collect())
    }
}

fn vector_de(texto: &str) -> Vec<f32> {
    let mut v = [0.1f32, 0.2, 0.3, 0.4];
    for (i, b) in texto.bytes().enumerate() {
        v[i % 4] += f32::from(b) / 1000.0;
    }
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
    v.into_iter().map(|x| x / norm).collect()
}

struct BaseRealista {
    db: PathBuf,
    bib: PathBuf,
    doc_contrato: PathBuf,
    doc_anexo: PathBuf,
    doc_boveda: PathBuf,
    clave_recuperacion: String,
    pin: &'static str,
}

/// Construye una base cifrada con información realista de prueba:
/// biblioteca indexada, subcarpetas, marcas `sensitive` y `vault`, paquete con
/// parte incluida, fecha de vencimiento corregida por el usuario, colección
/// creada por el usuario, documento en la papelera, fragmentos FTS5, vectores y PIN.
fn construir_base_realista(raiz: &Path) -> BaseRealista {
    let db = raiz.join("activa").join("brain.obrain");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();

    let bib = raiz.join("biblioteca_ficticia");
    let dir_contratos = bib.join("Contratos");
    let dir_identidad = bib.join("Identidad");
    let dir_nominas = bib.join("Nominas");
    let dir_temporal = bib.join("Temporal");
    for d in [&dir_contratos, &dir_identidad, &dir_nominas, &dir_temporal] {
        std::fs::create_dir_all(d).unwrap();
    }

    let parrafo_compartido = "Clausula primera: el arrendamiento sintetico de prueba incluye \
        mantenimiento integral del servidor local durante veinticuatro meses consecutivos \
        bajo tarifa fija auditada.\n\n\
        Clausula segunda: las partes acuerdan renovar el soporte antes del 2026-11-30 \
        mediante notificacion escrita en el registro interno.\n\n\
        Clausula tercera: los informes tecnicos mensuales se archivan de manera cifrada \
        en la boveda documental con verificacion criptografica.";

    let doc_anexo = dir_contratos.join("anexo_soporte.txt");
    std::fs::write(&doc_anexo, parrafo_compartido).unwrap();

    let doc_contrato = dir_contratos.join("expediente_completo.txt");
    std::fs::write(
        &doc_contrato,
        format!(
            "EXPEDIENTE MARCO SINTETICO 2026 — DOCUMENTO PRINCIPAL DE AUDITORIA Y CONTRATACION\n\n\
             Preambulo general: este expediente reune los anexos de soporte operativo y las \
             condiciones generales aplicables al centro de procesamiento de datos.\n\n\
             {parrafo_compartido}\n\n\
             Clausula cuarta: jurisdiccion sintetica de arbitraje tecnico y resolucion de \
             controversias conforme al reglamento interno vigente."
        ),
    )
    .unwrap();

    let doc_boveda = dir_identidad.join("pasaporte_sintetico.txt");
    std::fs::write(
        &doc_boveda,
        format!("{SECRETO_BOVEDA}\nDocumento ficticio de identidad para ensayo de respaldo."),
    )
    .unwrap();

    let doc_nomina = dir_nominas.join("nomina_enero.txt");
    std::fs::write(
        &doc_nomina,
        format!("{SECRETO_SENSIBLE}\nRetribucion bruta mensual de prueba."),
    )
    .unwrap();

    let doc_borrar = dir_temporal.join("borrador_efimero.txt");
    std::fs::write(
        &doc_borrar,
        "Borrador temporal sintetico que pasara a la papelera de treinta dias.",
    )
    .unwrap();

    let mut store = abrir_con(&db, K).unwrap();
    Brain::initialize_store(&mut store).unwrap();
    let modelo = ModeloFicticio;
    let rep1 = indexar::index(&mut store, std::slice::from_ref(&bib), Some(&modelo)).unwrap();
    assert_eq!(rep1.files_seen, 5);
    assert!(rep1.vectors_new > 0);

    // Enviar `borrador_efimero.txt` a la papelera y reindexar.
    std::fs::remove_file(&doc_borrar).unwrap();
    let rep2 = indexar::reindex(&mut store, Some(&modelo)).unwrap();
    assert_eq!(rep2.trashed, 1);

    let mut brain = Brain::try_from_store(store).unwrap();
    let clave_recuperacion = brain.recovery_code().unwrap().unwrap();
    brain.confirm_recovery_saved().unwrap();

    // Marcar carpetas: Identidad como bóveda (`vault`) y Nominas como `sensitive`.
    let ruta_identidad = std::path::absolute(&dir_identidad)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let id_identidad = brain.folder_at(&ruta_identidad).unwrap().unwrap();
    brain
        .mark_folder(&id_identidad, Some(Sensitivity::Vault))
        .unwrap();

    let ruta_nominas = std::path::absolute(&dir_nominas)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let id_nominas = brain.folder_at(&ruta_nominas).unwrap().unwrap();
    brain
        .mark_folder(&id_nominas, Some(Sensitivity::Sensitive))
        .unwrap();

    // Crear colección del usuario y meter documentos.
    let col_id = EntityId(
        brain
            .create_collection("Auditoria Sintetica", "user@test")
            .unwrap(),
    );
    let ruta_contrato = std::path::absolute(&doc_contrato)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let id_contrato = brain.document_at(&ruta_contrato).unwrap().unwrap();
    let ruta_anexo = std::path::absolute(&doc_anexo)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let id_anexo = brain.document_at(&ruta_anexo).unwrap().unwrap();
    brain
        .add_to_collection(&col_id, &[id_contrato.clone(), id_anexo], "user@test")
        .unwrap();

    // Fijar fecha de caducidad por el usuario en el contrato.
    brain.set_expiry(&id_contrato, Some("2026-12-15")).unwrap();

    // Configurar PIN.
    let pin = "2468";
    brain.set_pin(pin, None).unwrap();

    BaseRealista {
        db,
        bib,
        doc_contrato,
        doc_anexo,
        doc_boveda,
        clave_recuperacion,
        pin,
    }
}

fn contiene_bytes(ruta: &Path, aguja: &[u8]) -> bool {
    std::fs::read(ruta)
        .map(|b| b.windows(aguja.len()).any(|w| w == aguja))
        .unwrap_or(false)
}

#[test]
fn respaldo_de_base_cifrada_realista_conserva_todo_al_abrir() {
    let dir = CarpetaPrueba::nueva("realista");
    let fx = construir_base_realista(dir.path());
    let destino = dir.path().join("respaldo-01");

    let manifiesto = respaldo::crear(&fx.db, &destino).unwrap();

    // 1. Sólo existen los 3 archivos esperados; ni WAL, ni SHM, ni temporales.
    let mut archivos: Vec<String> = std::fs::read_dir(&destino)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    archivos.sort();
    assert_eq!(
        archivos,
        vec![
            BASE_ARCHIVO.to_string(),
            LLAVE_ARCHIVO.to_string(),
            MANIFIESTO_ARCHIVO.to_string()
        ]
    );

    // 2. Privacidad: ni la base respaldada ni el manifiesto exponen textos o rutas personales.
    let snap_db = destino.join(BASE_ARCHIVO);
    let snap_manifiesto = destino.join(MANIFIESTO_ARCHIVO);
    let snap_llave = destino.join(LLAVE_ARCHIVO);
    for archivo in [&snap_db, &snap_manifiesto, &snap_llave] {
        assert!(!contiene_bytes(archivo, b"PASAPORTE SINTETICO"));
        assert!(!contiene_bytes(archivo, b"NOMINA SINTETICA"));
        assert!(!contiene_bytes(archivo, b"Auditoria Sintetica"));
        assert!(!contiene_bytes(archivo, fx.clave_recuperacion.as_bytes()));
    }
    let texto_manifiesto = std::fs::read_to_string(&snap_manifiesto).unwrap();
    assert!(!texto_manifiesto.contains("biblioteca_ficticia"));
    assert!(!texto_manifiesto.contains("pasaporte_sintetico"));

    // 3. Verificación directa y con clave de recuperación.
    let verificado = respaldo::verificar(&destino).unwrap();
    assert_eq!(verificado, manifiesto);
    let verificado_clave =
        respaldo::verificar_con_clave(&destino, Some(&fx.clave_recuperacion)).unwrap();
    assert_eq!(verificado_clave, manifiesto);
    assert!(manifiesto.instantanea.tiene_pin);
    assert!(manifiesto.instantanea.tiene_recuperacion_interna);
    assert!(manifiesto.instantanea.fragmentos >= 5);
    assert!(manifiesto.instantanea.vectores >= 4);

    // 4. Ensayo en carpeta separada y apertura con Brain.
    let carpeta_ensayo = dir.path().join("ensayo-abierto");
    let informe = respaldo::ensayar(
        &destino,
        Some(&carpeta_ensayo),
        Some(&fx.clave_recuperacion),
        Some(fx.pin),
    )
    .unwrap();
    assert!(informe.recuperada_con_clave);
    assert!(informe.con_pin);
    assert!(informe.pin_desbloqueado);
    assert_eq!(informe.documentos_en_boveda, 4);
    assert_eq!(informe.en_papelera, 1);
    assert_eq!(informe.colecciones, 1);
    assert!(informe.sensibles >= 1);
    assert_eq!(informe.boveda, 1);
    assert_eq!(informe.bibliotecas_registradas.len(), 1);
    assert!(informe.bibliotecas_ausentes_en_disco.is_empty());

    // 5. Abrir la copia ensayada y comprobar bloqueo por PIN + conservación de datos.
    let mut brain = Brain::open(carpeta_ensayo.join(BASE_ARCHIVO)).unwrap();
    assert!(brain.warm(None).is_ok());
    let ruta_boveda = std::path::absolute(&fx.doc_boveda)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let id_boveda = brain.document_at(&ruta_boveda).unwrap().unwrap();

    // Antes de desbloquear el PIN, el contenido de bóveda sigue protegido.
    assert!(brain.preview(&id_boveda, None).unwrap().locked);
    assert!(matches!(brain.recovery_code(), Err(Error::Bloqueado)));

    // Al desbloquear con el PIN, todo el contenido, colecciones, paquetes y búsqueda están intactos.
    brain.unlock(fx.pin).unwrap();
    let prev = brain.preview(&id_boveda, None).unwrap();
    assert!(!prev.locked);
    assert!(prev.text.as_deref().unwrap().contains(SECRETO_BOVEDA));
    assert_eq!(
        brain.recovery_code().unwrap().as_deref(),
        Some(fx.clave_recuperacion.as_str())
    );

    let busq = brain.search("arrendamiento", 10).unwrap();
    assert!(!busq.hits.is_empty());

    let cols = brain.collections().unwrap();
    assert_eq!(cols.len(), 1);
    assert_eq!(cols[0].name, "Auditoria Sintetica");
    assert_eq!(cols[0].documents, 2);

    let paquetes = brain.packages().unwrap();
    assert_eq!(paquetes.len(), 1);
    assert_eq!(paquetes[0].parts.len(), 1);

    let papelera = brain.trash().unwrap();
    assert_eq!(papelera.len(), 1);

    let marcadas = brain.marked_folders().unwrap();
    assert_eq!(marcadas.len(), 2);
}

#[test]
fn instantanea_durante_actividad_concurrente_es_consistente() {
    let dir = CarpetaPrueba::nueva("concurrente");
    let db = dir.path().join("activa").join("brain.obrain");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();

    let mut store = abrir_con(&db, K).unwrap();
    Brain::initialize_store(&mut store).unwrap();

    // Estado base inicial (20 entidades con su aserción en la misma transacción).
    store
        .transaction(|s| -> obrain_core::Result<()> {
            for i in 0..20 {
                let e = s.create_entity(100 + i)?;
                s.assert(NewAssertion::new(
                    e,
                    "has_facet",
                    format!("inicial-{i}"),
                    Method::User,
                    100 + i,
                ))?;
            }
            Ok(())
        })
        .unwrap();

    // Mantener un lector con una instantánea abierta para impedir que el WAL se trunque,
    // mientras un hilo escritor añade transacciones atómicas (entidad + 2 aserciones).
    let reader = Store::open(&db).unwrap();
    reader.conn().execute_batch("BEGIN DEFERRED").unwrap();
    let _: i64 = reader
        .conn()
        .query_row("SELECT count(*) FROM entities", [], |r| r.get(0))
        .unwrap();

    let db_writer = db.clone();
    let detener = Arc::new(AtomicBool::new(false));
    let detener_hilo = Arc::clone(&detener);
    let escritor = std::thread::spawn(move || {
        let w = Store::open(&db_writer).unwrap();
        let mut seq = 1000i64;
        while !detener_hilo.load(Ordering::Relaxed) {
            let _ = w.atomic_write(|s| -> obrain_core::Result<()> {
                let e = s.create_entity(seq)?;
                s.assert(NewAssertion::new(
                    e.clone(),
                    "about",
                    format!("valor-{seq}"),
                    Method::User,
                    seq,
                ))?;
                s.assert(NewAssertion::new(
                    e,
                    "mentions",
                    format!("espejo-{seq}"),
                    Method::User,
                    seq,
                ))?;
                Ok(())
            });
            seq += 1;
        }
    });

    // Dar margen para que entren transacciones nuevas en el WAL antes y durante el respaldo.
    std::thread::sleep(std::time::Duration::from_millis(40));

    let respaldo_dir = dir.path().join("respaldo-concurrente");
    let manifiesto = respaldo::crear(&db, &respaldo_dir).unwrap();

    // Seguir escribiendo un poco más tras el respaldo para demostrar que la base
    // activa avanza sin invalidar la coherencia interna del respaldo tomado.
    std::thread::sleep(std::time::Duration::from_millis(30));
    detener.store(true, Ordering::Relaxed);
    escritor.join().unwrap();
    reader.conn().execute_batch("COMMIT").unwrap();

    // El respaldo publicado es verificado e internamente consistente:
    let verificado = respaldo::verificar(&respaldo_dir).unwrap();
    assert_eq!(verificado, manifiesto);
    assert!(manifiesto.instantanea.entidades >= 20);

    // Ninguna transacción atómica (`about` + `mentions`) pudo quedar partida a la mitad.
    let snap = Store::open(respaldo_dir.join(BASE_ARCHIVO)).unwrap();
    let pares_a = snap
        .query(None, Some("about"), TimePoint::at(i64::MAX))
        .unwrap()
        .len();
    let pares_b = snap
        .query(None, Some("mentions"), TimePoint::at(i64::MAX))
        .unwrap()
        .len();
    assert_eq!(
        pares_a, pares_b,
        "la instantánea capturó una transacción a medias"
    );
}

#[test]
fn detecta_archivos_ausentes_incompletos_modificados_o_desparejados() {
    let dir = CarpetaPrueba::nueva("integridad");
    let fx = construir_base_realista(dir.path());
    let base_respaldo = dir.path().join("respaldo-base");
    let manifiesto = respaldo::crear(&fx.db, &base_respaldo).unwrap();

    // No sobrescribe respaldos existentes ni carpetas ocupadas.
    assert!(matches!(
        respaldo::crear(&fx.db, &base_respaldo),
        Err(RespaldoError::DestinoOcupado(_))
    ));
    assert!(matches!(
        respaldo::crear(&fx.db, fx.db.parent().unwrap()),
        Err(RespaldoError::DestinoInvalido(_))
    ));

    let clonar_respaldo = |nombre: &str| -> PathBuf {
        let copia = dir.path().join(nombre);
        std::fs::create_dir_all(&copia).unwrap();
        for f in [BASE_ARCHIVO, LLAVE_ARCHIVO, MANIFIESTO_ARCHIVO] {
            std::fs::copy(base_respaldo.join(f), copia.join(f)).unwrap();
        }
        copia
    };

    // Caso A: falta cada uno de los tres archivos.
    for faltante in [BASE_ARCHIVO, LLAVE_ARCHIVO, MANIFIESTO_ARCHIVO] {
        let r = clonar_respaldo(&format!("falta-{faltante}"));
        std::fs::remove_file(r.join(faltante)).unwrap();
        assert!(matches!(
            respaldo::verificar(&r),
            Err(RespaldoError::FaltaArchivo(_))
        ));
    }

    // Caso B: archivo vacío (0 bytes).
    for vacio in [BASE_ARCHIVO, LLAVE_ARCHIVO, MANIFIESTO_ARCHIVO] {
        let r = clonar_respaldo(&format!("vacio-{vacio}"));
        std::fs::write(r.join(vacio), b"").unwrap();
        assert!(matches!(
            respaldo::verificar(&r),
            Err(RespaldoError::ArchivoVacio(_))
        ));
    }

    // Caso C: archivo auxiliar inesperado (`-wal`, `.cifrando`, archivo extraño).
    for extra in ["brain.obrain-wal", "brain.obrain.cifrando", "nota.txt"] {
        let r = clonar_respaldo(&format!("extra-{extra}"));
        std::fs::write(r.join(extra), b"basura").unwrap();
        assert!(matches!(
            respaldo::verificar(&r),
            Err(RespaldoError::ArchivoInesperado(_))
        ));
    }

    // Caso D: base truncada.
    {
        let r = clonar_respaldo("base-truncada");
        let bytes = std::fs::read(r.join(BASE_ARCHIVO)).unwrap();
        std::fs::write(r.join(BASE_ARCHIVO), &bytes[..bytes.len() / 2]).unwrap();
        assert!(matches!(
            respaldo::verificar(&r),
            Err(RespaldoError::TamanoNoCoincide { .. })
        ));
    }

    // Caso E: byte alterado en `brain.obrain` manteniendo el tamaño.
    {
        let r = clonar_respaldo("base-alterada");
        let mut bytes = std::fs::read(r.join(BASE_ARCHIVO)).unwrap();
        let idx = bytes.len() / 2;
        bytes[idx] ^= 0x55;
        std::fs::write(r.join(BASE_ARCHIVO), &bytes).unwrap();
        assert!(matches!(
            respaldo::verificar(&r),
            Err(RespaldoError::HashNoCoincide { .. })
        ));
    }

    // Caso F: manifiesto corrupto o versión incompatible.
    {
        let r = clonar_respaldo("manifiesto-corrupto");
        std::fs::write(r.join(MANIFIESTO_ARCHIVO), "{ json roto").unwrap();
        assert!(matches!(
            respaldo::verificar(&r),
            Err(RespaldoError::ManifiestoInvalido(_))
        ));

        let mut m_futuro = manifiesto.clone();
        m_futuro.version = 99;
        std::fs::write(
            r.join(MANIFIESTO_ARCHIVO),
            serde_json::to_string_pretty(&m_futuro).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            respaldo::verificar(&r),
            Err(RespaldoError::ManifiestoInvalido(_))
        ));
    }

    // Caso G: `.llave` de otra base (incluso si alguien falsificara el hash en el manifiesto).
    {
        let otra_dir = dir.path().join("otra-base");
        std::fs::create_dir_all(&otra_dir).unwrap();
        let otra_db = otra_dir.join("brain.obrain");
        let _ = abrir_con(&otra_db, K).unwrap();

        let r = clonar_respaldo("llave-cruzada");
        std::fs::copy(obrain_keys::key_path(&otra_db), r.join(LLAVE_ARCHIVO)).unwrap();
        // Sin tocar el manifiesto, falla por hash/tamaño de `.llave`.
        assert!(matches!(
            respaldo::verificar(&r),
            Err(RespaldoError::HashNoCoincide { .. } | RespaldoError::TamanoNoCoincide { .. })
        ));

        // Incluso recalculando hashes en el manifiesto, la llave no abre la base.
        let llave_bytes = std::fs::read(r.join(LLAVE_ARCHIVO)).unwrap();
        let canon = obrain_keys::recovery_box_canonical(&r.join(BASE_ARCHIVO)).unwrap();
        let mut m_falso: Manifiesto =
            serde_json::from_str(&std::fs::read_to_string(r.join(MANIFIESTO_ARCHIVO)).unwrap())
                .unwrap();
        m_falso.llave.bytes = llave_bytes.len() as u64;
        m_falso.llave.blake3 = blake3::hash(&llave_bytes).to_hex().to_string();
        m_falso.llave.huella_recuperacion = blake3::hash(canon.as_bytes()).to_hex().to_string();
        std::fs::write(
            r.join(MANIFIESTO_ARCHIVO),
            serde_json::to_string_pretty(&m_falso).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            respaldo::verificar(&r),
            Err(RespaldoError::LlaveNoCorresponde(_))
        ));
    }

    // Caso H: manifiesto con hash de archivo válido pero `instantanea` de otro momento.
    {
        let r = clonar_respaldo("instantanea-desfasada");
        let mut m_desfasado = manifiesto;
        m_desfasado.instantanea.entidades += 1;
        std::fs::write(
            r.join(MANIFIESTO_ARCHIVO),
            serde_json::to_string_pretty(&m_desfasado).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            respaldo::verificar(&r),
            Err(RespaldoError::InstantaneaNoCoincide(_))
        ));
    }
}

#[test]
fn clave_de_recuperacion_o_pin_incorrectos_fallan_sin_alterar_archivos() {
    let dir = CarpetaPrueba::nueva("credenciales");
    let fx = construir_base_realista(dir.path());
    let respaldo_dir = dir.path().join("respaldo-credenciales");
    respaldo::crear(&fx.db, &respaldo_dir).unwrap();

    let hash_antes = |archivo: &str| -> String {
        let bytes = std::fs::read(respaldo_dir.join(archivo)).unwrap();
        blake3::hash(&bytes).to_hex().to_string()
    };
    let h_base = hash_antes(BASE_ARCHIVO);
    let h_llave = hash_antes(LLAVE_ARCHIVO);
    let h_man = hash_antes(MANIFIESTO_ARCHIVO);

    // 1. Clave mal formada.
    assert!(matches!(
        respaldo::verificar_con_clave(&respaldo_dir, Some("no-es-una-clave")),
        Err(RespaldoError::Keys(obrain_keys::Error::CodigoMalFormado))
    ));

    // 2. Clave con formato válido pero distinta.
    let otra_clave = obrain_keys::RecoveryCode::generate().unwrap().to_string();
    assert!(matches!(
        respaldo::verificar_con_clave(&respaldo_dir, Some(&otra_clave)),
        Err(RespaldoError::Keys(obrain_keys::Error::ClaveIncorrecta))
    ));

    let destino_fallido = dir.path().join("ensayo-no-debe-existir");
    assert!(matches!(
        respaldo::ensayar(
            &respaldo_dir,
            Some(&destino_fallido),
            Some(&otra_clave),
            None
        ),
        Err(RespaldoError::Keys(obrain_keys::Error::ClaveIncorrecta))
    ));
    assert!(!destino_fallido.exists());

    // 3. PIN incorrecto durante el ensayo.
    assert!(matches!(
        respaldo::ensayar(
            &respaldo_dir,
            Some(&destino_fallido),
            Some(&fx.clave_recuperacion),
            Some("9999")
        ),
        Err(RespaldoError::Seguridad(seguridad::Error::PinIncorrecto))
    ));
    assert!(!destino_fallido.exists());

    // Los archivos del respaldo permanecen intactos byte a byte.
    assert_eq!(hash_antes(BASE_ARCHIVO), h_base);
    assert_eq!(hash_antes(LLAVE_ARCHIVO), h_llave);
    assert_eq!(hash_antes(MANIFIESTO_ARCHIVO), h_man);
}

#[test]
fn interrupcion_durante_generacion_no_publica_respaldo_ni_deja_temporales() {
    let dir = CarpetaPrueba::nueva("interrupcion");
    let fx = construir_base_realista(dir.path());
    let contenedor = dir.path().join("contenedor-respaldos");
    std::fs::create_dir_all(&contenedor).unwrap();

    for etapa in [
        EtapaRespaldo::TrasCopiarLlave,
        EtapaRespaldo::TrasExportarBase,
        EtapaRespaldo::TrasEscribirManifiesto,
        EtapaRespaldo::AntesDePublicar,
    ] {
        let destino = contenedor.join(format!("respaldo-{etapa:?}"));
        let res = respaldo::crear_con_etapa(&fx.db, &destino, |actual| {
            if actual == etapa {
                return Err(RespaldoError::Interrumpido(format!("{etapa:?}")));
            }
            Ok(())
        });
        assert!(matches!(res, Err(RespaldoError::Interrumpido(_))));
        assert!(
            !destino.exists(),
            "no debe publicarse la carpeta de respaldo si se interrumpe en {etapa:?}"
        );

        // Tampoco deben quedar directorios `.obrain-respaldo-tmp-*` en el contenedor.
        let entradas: Vec<String> = std::fs::read_dir(&contenedor)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            entradas.is_empty(),
            "quedaron archivos temporales tras interrumpir en {etapa:?}: {entradas:?}"
        );
    }

    // Tras las interrupciones, generar el respaldo completo en el mismo destino funciona.
    let destino_final = contenedor.join("respaldo-completo");
    respaldo::crear(&fx.db, &destino_final).unwrap();
    respaldo::verificar(&destino_final).unwrap();
}

#[test]
fn ensayo_aislado_en_otro_equipo_sin_dpapi_no_modifica_respaldo_ni_base_activa() {
    let dir = CarpetaPrueba::nueva("otro-equipo");
    let fx = construir_base_realista(dir.path());

    // Simular un respaldo traído de otro equipo cuya caja `sistema` (DPAPI) no se
    // puede abrir en la máquina actual, pero cuya caja `recuperacion` es legítima.
    let respaldo_otro_equipo = dir.path().join("respaldo-otro-equipo");
    respaldo::crear(&fx.db, &respaldo_otro_equipo).unwrap();

    let llave_path = respaldo_otro_equipo.join(LLAVE_ARCHIVO);
    let otro_sistema = if cfg!(windows) {
        "sin-proteger"
    } else {
        "dpapi"
    };
    let llave_mod = std::fs::read_to_string(&llave_path).unwrap().replace(
        if cfg!(windows) {
            "\"tipo\": \"dpapi\""
        } else {
            "\"tipo\": \"sin-proteger\""
        },
        &format!("\"tipo\": \"{otro_sistema}\""),
    );
    std::fs::write(&llave_path, &llave_mod).unwrap();

    // Actualizar el hash del archivo `.llave` en el manifiesto (como habría salido
    // originalmente de ese otro equipo).
    let man_path = respaldo_otro_equipo.join(MANIFIESTO_ARCHIVO);
    let mut man: Manifiesto =
        serde_json::from_str(&std::fs::read_to_string(&man_path).unwrap()).unwrap();
    man.llave.bytes = llave_mod.len() as u64;
    man.llave.blake3 = blake3::hash(llave_mod.as_bytes()).to_hex().to_string();
    std::fs::write(&man_path, serde_json::to_string_pretty(&man).unwrap()).unwrap();

    let hash_activa_antes = blake3::hash(&std::fs::read(&fx.db).unwrap())
        .to_hex()
        .to_string();
    let hash_llave_respaldo_antes = blake3::hash(&std::fs::read(&llave_path).unwrap())
        .to_hex()
        .to_string();

    // 1. Sin clave de recuperación, `verificar` indica que el sistema no puede abrir la llave.
    let err_sin_clave = respaldo::verificar(&respaldo_otro_equipo).unwrap_err();
    assert!(obrain_api::Error::from(err_sin_clave).is_key_unavailable());

    // 2. Con la clave de recuperación, `verificar_con_clave` valida todo sin tocar `.llave`.
    respaldo::verificar_con_clave(&respaldo_otro_equipo, Some(&fx.clave_recuperacion)).unwrap();
    assert_eq!(
        blake3::hash(&std::fs::read(&llave_path).unwrap())
            .to_hex()
            .to_string(),
        hash_llave_respaldo_antes
    );

    // 3. Ensayo temporal (sin `--en`) y ensayo persistido (`--en`): ambos reenvuelven la llave
    //    en la copia aislada sin tocar ni el respaldo ni la base activa.
    let inf_temp = respaldo::ensayar(
        &respaldo_otro_equipo,
        None,
        Some(&fx.clave_recuperacion),
        Some(fx.pin),
    )
    .unwrap();
    assert!(inf_temp.carpeta_ensayo.is_none());
    assert!(inf_temp.recuperada_con_clave);
    assert!(inf_temp.pin_desbloqueado);

    let destino_ensayo = dir.path().join("ensayo-recuperado");
    let inf_en = respaldo::ensayar(
        &respaldo_otro_equipo,
        Some(&destino_ensayo),
        Some(&fx.clave_recuperacion),
        Some(fx.pin),
    )
    .unwrap();
    assert_eq!(
        inf_en.carpeta_ensayo.as_deref(),
        Some(std::path::absolute(&destino_ensayo).unwrap().as_path())
    );

    // La copia de ensayo ya abre por sí sola en este equipo sin volver a pedir la clave.
    let b_ensayo = Brain::open(destino_ensayo.join(BASE_ARCHIVO)).unwrap();
    b_ensayo.unlock(fx.pin).unwrap();
    assert_eq!(b_ensayo.recent(10).unwrap().len(), 4);

    // Ni la base activa ni la llave del respaldo cambiaron.
    assert_eq!(
        blake3::hash(&std::fs::read(&fx.db).unwrap())
            .to_hex()
            .to_string(),
        hash_activa_antes
    );
    assert_eq!(
        blake3::hash(&std::fs::read(&llave_path).unwrap())
            .to_hex()
            .to_string(),
        hash_llave_respaldo_antes
    );
}

#[test]
fn comportamiento_cuando_faltan_los_documentos_originales_externos() {
    let dir = CarpetaPrueba::nueva("documentos-externos");
    let fx = construir_base_realista(dir.path());
    let respaldo_dir = dir.path().join("respaldo-externos");
    respaldo::crear(&fx.db, &respaldo_dir).unwrap();

    let ruta_contrato = std::path::absolute(&fx.doc_contrato)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let ruta_anexo = std::path::absolute(&fx.doc_anexo)
        .unwrap()
        .to_string_lossy()
        .into_owned();

    // ── Caso 1: falta toda la carpeta de la biblioteca original (p. ej. otro equipo).
    let bib_respaldada = dir.path().join("biblioteca_movida_temporalmente");
    std::fs::rename(&fx.bib, &bib_respaldada).unwrap();

    let ensayo_sin_bib = dir.path().join("ensayo-sin-biblioteca");
    let inf = respaldo::ensayar(
        &respaldo_dir,
        Some(&ensayo_sin_bib),
        Some(&fx.clave_recuperacion),
        Some(fx.pin),
    )
    .unwrap();
    assert_eq!(inf.bibliotecas_registradas.len(), 1);
    assert_eq!(inf.bibliotecas_presentes, 0);
    assert_eq!(inf.bibliotecas_ausentes_en_disco.len(), 1);
    assert_eq!(inf.documentos_en_boveda, 4);

    {
        let mut store_ensayo = seguridad::abrir(&ensayo_sin_bib.join(BASE_ARCHIVO)).unwrap();
        Brain::initialize_store(&mut store_ensayo).unwrap();
        // Al reindexar cuando la raíz de la biblioteca no existe en disco, se anota
        // en `missing` y NO se manda ningún documento a la papelera.
        let rep = indexar::reindex(&mut store_ensayo, None).unwrap();
        assert_eq!(rep.missing.len(), 1);
        assert_eq!(rep.trashed, 0);

        let b = Brain::try_from_store(store_ensayo).unwrap();
        b.unlock(fx.pin).unwrap();
        // La búsqueda léxica, los fragmentos indexados (`preview`), las colecciones
        // y los vencimientos siguen funcionando desde SQLite; abrir el archivo externo
        // (`path_to_open`) falla cerrado (`Ok(None)`).
        let hits = b.search("arrendamiento", 10).unwrap().hits;
        assert!(!hits.is_empty());
        let id_contrato = b.document_at(&ruta_contrato).unwrap().unwrap();
        let prev = b.preview(&id_contrato, None).unwrap();
        assert!(
            prev.text
                .as_deref()
                .unwrap()
                .contains("EXPEDIENTE MARCO SINTETICO")
        );
        assert_eq!(b.path_to_open(&id_contrato).unwrap(), None);
        assert_eq!(b.collections().unwrap()[0].documents, 2);
    }

    // ── Caso 2: la carpeta de la biblioteca existe, pero falta un archivo individual;
    //    al reindexar pasa a la papelera de 30 días sin perder colecciones ni aserciones,
    //    y si el archivo externo se restaura vuelve a la bóveda.
    std::fs::rename(&bib_respaldada, &fx.bib).unwrap();
    let bytes_anexo = std::fs::read(&fx.doc_anexo).unwrap();
    std::fs::remove_file(&fx.doc_anexo).unwrap();

    let ensayo_archivo_ausente = dir.path().join("ensayo-archivo-ausente");
    respaldo::ensayar(
        &respaldo_dir,
        Some(&ensayo_archivo_ausente),
        Some(&fx.clave_recuperacion),
        Some(fx.pin),
    )
    .unwrap();
    {
        let mut store_ensayo =
            seguridad::abrir(&ensayo_archivo_ausente.join(BASE_ARCHIVO)).unwrap();
        Brain::initialize_store(&mut store_ensayo).unwrap();
        let rep_ausente = indexar::reindex(&mut store_ensayo, None).unwrap();
        assert_eq!(rep_ausente.trashed, 1);

        let b = Brain::try_from_store(store_ensayo).unwrap();
        let cols = b.collections().unwrap();
        assert_eq!(cols[0].documents, 1);
        assert_eq!(cols[0].in_trash, 1);
    }

    // Restaurar el archivo externo en disco y volver a reindexar la copia ensayada.
    std::fs::write(&fx.doc_anexo, &bytes_anexo).unwrap();
    {
        let mut store_ensayo =
            seguridad::abrir(&ensayo_archivo_ausente.join(BASE_ARCHIVO)).unwrap();
        Brain::initialize_store(&mut store_ensayo).unwrap();
        let rep_vuelta = indexar::reindex(&mut store_ensayo, None).unwrap();
        assert_eq!(rep_vuelta.returned, 1);

        let b = Brain::try_from_store(store_ensayo).unwrap();
        let cols = b.collections().unwrap();
        assert_eq!(cols[0].documents, 2);
        assert_eq!(cols[0].in_trash, 0);
        assert!(b.document_at(&ruta_anexo).unwrap().is_some());
    }
}

#[test]
fn cli_obrain_respaldar_verificar_y_ensayar_extremo_a_extremo() {
    let dir = CarpetaPrueba::nueva("cli");
    let fx = construir_base_realista(dir.path());
    let bin = env!("CARGO_BIN_EXE_obrain");

    let respaldo_cli = dir.path().join("respaldo-cli");
    let out_crear = Command::new(bin)
        .args([
            "--db",
            fx.db.to_str().unwrap(),
            "respaldar",
            respaldo_cli.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out_crear.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out_crear.stderr)
    );
    let stdout_crear = String::from_utf8_lossy(&out_crear.stdout);
    assert!(stdout_crear.contains("respaldo creado y verificado"));

    // Repetir sobre la misma carpeta debe fallar sin sobrescribir.
    let out_dup = Command::new(bin)
        .args([
            "--db",
            fx.db.to_str().unwrap(),
            "respaldar",
            respaldo_cli.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!out_dup.status.success());

    // Verificar con clave desde la CLI (usando una ruta `--db` inexistente para
    // comprobar que `--verificar` no crea ni toca ninguna base activa).
    let db_inexistente = dir.path().join("no-crear").join("brain.obrain");
    let out_ver = Command::new(bin)
        .args([
            "--db",
            db_inexistente.to_str().unwrap(),
            "respaldo",
            respaldo_cli.to_str().unwrap(),
            "--verificar",
            "--clave",
            &fx.clave_recuperacion,
        ])
        .output()
        .unwrap();
    assert!(
        out_ver.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out_ver.stderr)
    );
    assert!(
        !db_inexistente.parent().unwrap().exists(),
        "verificar no debe crear el directorio de la base activa"
    );

    // Ensayar con `--en`, `--clave` y `--pin` desde la CLI.
    let ensayo_cli = dir.path().join("ensayo-cli");
    let out_ens = Command::new(bin)
        .args([
            "--db",
            db_inexistente.to_str().unwrap(),
            "respaldo",
            respaldo_cli.to_str().unwrap(),
            "--ensayar",
            "--en",
            ensayo_cli.to_str().unwrap(),
            "--clave",
            &fx.clave_recuperacion,
            "--pin",
            fx.pin,
        ])
        .output()
        .unwrap();
    assert!(
        out_ens.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out_ens.stderr)
    );
    let stdout_ens = String::from_utf8_lossy(&out_ens.stdout);
    assert!(stdout_ens.contains("ensayo de recuperación satisfactorio"));
    assert!(ensayo_cli.join(BASE_ARCHIVO).is_file());
    assert!(!db_inexistente.parent().unwrap().exists());
}

#[test]
fn directorio_temporal_abandonado_se_rechaza_y_no_bloquea_nuevo_respaldo() {
    let dir = CarpetaPrueba::nueva("staging-abandonado");
    let fx = construir_base_realista(dir.path());

    // 1. Crear un respaldo legítimo previo para tener archivos válidos.
    let respaldo_valido = dir.path().join("respaldo-valido");
    respaldo::crear(&fx.db, &respaldo_valido).unwrap();

    // 2. Simular un directorio temporal abandonado (corte de luz/kill sin Drop)
    //    con archivos válidos en su interior.
    let staging_abandonado = dir
        .path()
        .join(".obrain-respaldo-tmp-deadbeef1234567890abcdef");
    std::fs::create_dir_all(&staging_abandonado).unwrap();
    std::fs::copy(
        respaldo_valido.join(BASE_ARCHIVO),
        staging_abandonado.join(BASE_ARCHIVO),
    )
    .unwrap();
    std::fs::copy(
        respaldo_valido.join(LLAVE_ARCHIVO),
        staging_abandonado.join(LLAVE_ARCHIVO),
    )
    .unwrap();
    std::fs::copy(
        respaldo_valido.join(MANIFIESTO_ARCHIVO),
        staging_abandonado.join(MANIFIESTO_ARCHIVO),
    )
    .unwrap();

    // El sistema debe negarse a verificar un directorio temporal abandonado.
    let err_ver = respaldo::verificar(&staging_abandonado).unwrap_err();
    match err_ver {
        RespaldoError::DestinoInvalido(msg) => {
            assert!(msg.contains("directorio temporal"));
        }
        otro => panic!("se esperaba DestinoInvalido, se obtuvo: {otro:?}"),
    }

    // Tampoco se debe permitir crear ni ensayar directamente contra una ruta reservada.
    let destino_prohibido = dir.path().join(".obrain-respaldo-tmp-intento");
    let err_crear = respaldo::crear(&fx.db, &destino_prohibido).unwrap_err();
    assert!(matches!(err_crear, RespaldoError::DestinoInvalido(_)));

    let ensayo_prohibido = dir.path().join(".obrain-ensayo-tmp-intento");
    let err_ens =
        respaldo::ensayar(&respaldo_valido, Some(&ensayo_prohibido), None, None).unwrap_err();
    assert!(matches!(err_ens, RespaldoError::DestinoInvalido(_)));

    // 3. La presencia del temporal abandonado en el mismo directorio padre no debe
    //    impedir crear un nuevo respaldo legítimo en otra ruta.
    let nuevo_respaldo = dir.path().join("segundo-respaldo");
    let mani = respaldo::crear(&fx.db, &nuevo_respaldo).unwrap();
    assert_eq!(mani.version, 1);
    assert!(nuevo_respaldo.join(BASE_ARCHIVO).is_file());
}

#[test]
fn cli_obrain_secretos_por_stdin_y_variables_de_entorno() {
    use std::io::Write as _;
    use std::process::Stdio;

    let dir = CarpetaPrueba::nueva("cli-secretos");
    let fx = construir_base_realista(dir.path());
    let bin = env!("CARGO_BIN_EXE_obrain");

    let respaldo_dir = dir.path().join("respaldo-cli");
    let out_crear = Command::new(bin)
        .args([
            "--db",
            fx.db.to_str().unwrap(),
            "respaldar",
            respaldo_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out_crear.status.success());

    // 1. --verificar con --clave - pasando la clave por entrada estándar (stdin).
    let mut proc_ver = Command::new(bin)
        .args([
            "respaldo",
            respaldo_dir.to_str().unwrap(),
            "--verificar",
            "--clave",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut stdin = proc_ver.stdin.take().unwrap();
        writeln!(stdin, "{}", fx.clave_recuperacion).unwrap();
    }
    let out_ver = proc_ver.wait_with_output().unwrap();
    assert!(
        out_ver.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out_ver.stderr)
    );
    let txt_ver = String::from_utf8_lossy(&out_ver.stdout);
    assert!(txt_ver.contains("clave de recuperación  verificada"));

    // 2. --ensayar con --clave - y --pin - pasando ambos por entrada estándar.
    let ensayo_stdin = dir.path().join("ensayo-stdin");
    let mut proc_ens = Command::new(bin)
        .args([
            "respaldo",
            respaldo_dir.to_str().unwrap(),
            "--ensayar",
            "--en",
            ensayo_stdin.to_str().unwrap(),
            "--clave",
            "-",
            "--pin",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut stdin = proc_ens.stdin.take().unwrap();
        writeln!(stdin, "{}", fx.clave_recuperacion).unwrap();
        writeln!(stdin, "{}", fx.pin).unwrap();
    }
    let out_ens = proc_ens.wait_with_output().unwrap();
    assert!(
        out_ens.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out_ens.stderr)
    );
    let txt_ens = String::from_utf8_lossy(&out_ens.stdout);
    assert!(txt_ens.contains("ensayo de recuperación satisfactorio"));
    assert!(txt_ens.contains("activo y verificado en el ensayo"));

    // 3. --verificar utilizando la variable de entorno OBRAIN_CLAVE sin exponerla en argumentos.
    let out_env_ver = Command::new(bin)
        .args(["respaldo", respaldo_dir.to_str().unwrap(), "--verificar"])
        .env("OBRAIN_CLAVE", &fx.clave_recuperacion)
        .output()
        .unwrap();
    assert!(
        out_env_ver.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out_env_ver.stderr)
    );
    let txt_env_ver = String::from_utf8_lossy(&out_env_ver.stdout);
    assert!(txt_env_ver.contains("clave de recuperación  verificada"));

    // 4. --ensayar utilizando las variables de entorno OBRAIN_CLAVE y OBRAIN_PIN.
    let ensayo_env = dir.path().join("ensayo-env");
    let out_env_ens = Command::new(bin)
        .args([
            "respaldo",
            respaldo_dir.to_str().unwrap(),
            "--ensayar",
            "--en",
            ensayo_env.to_str().unwrap(),
        ])
        .env("OBRAIN_CLAVE", &fx.clave_recuperacion)
        .env("OBRAIN_PIN", fx.pin)
        .output()
        .unwrap();
    assert!(
        out_env_ens.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out_env_ens.stderr)
    );
    let txt_env_ens = String::from_utf8_lossy(&out_env_ens.stdout);
    assert!(txt_env_ens.contains("ensayo de recuperación satisfactorio"));
    assert!(txt_env_ens.contains("activo y verificado en el ensayo"));

    // 5. obrain recuperar - pasando la clave por stdin.
    let db_rec = dir.path().join("base-recuperar").join("brain.obrain");
    std::fs::create_dir_all(db_rec.parent().unwrap()).unwrap();
    std::fs::copy(&fx.db, &db_rec).unwrap();
    let llave_rec = obrain_keys::key_path(&db_rec);
    std::fs::copy(obrain_keys::key_path(&fx.db), &llave_rec).unwrap();

    // Invalidar DPAPI en la llave para requerir recuperación.
    let propio = if cfg!(windows) {
        "dpapi"
    } else {
        "sin-proteger"
    };
    let ajeno = if cfg!(windows) {
        "sin-proteger"
    } else {
        "dpapi"
    };
    let texto_llave = std::fs::read_to_string(&llave_rec)
        .unwrap()
        .replace(propio, ajeno);
    std::fs::write(&llave_rec, &texto_llave).unwrap();

    let mut proc_rec = Command::new(bin)
        .args(["--db", db_rec.to_str().unwrap(), "recuperar", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut stdin = proc_rec.stdin.take().unwrap();
        writeln!(stdin, "{}", fx.clave_recuperacion).unwrap();
    }
    let out_rec = proc_rec.wait_with_output().unwrap();
    assert!(
        out_rec.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out_rec.stderr)
    );
    let txt_rec = String::from_utf8_lossy(&out_rec.stdout);
    assert!(txt_rec.contains("la base vuelve a abrirse"));

    // 6. obrain recuperar utilizando la variable OBRAIN_CLAVE.
    std::fs::write(&llave_rec, texto_llave).unwrap();
    let out_rec_env = Command::new(bin)
        .args(["--db", db_rec.to_str().unwrap(), "recuperar"])
        .env("OBRAIN_CLAVE", &fx.clave_recuperacion)
        .output()
        .unwrap();
    assert!(
        out_rec_env.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out_rec_env.stderr)
    );
    let txt_rec_env = String::from_utf8_lossy(&out_rec_env.stdout);
    assert!(txt_rec_env.contains("la base vuelve a abrirse"));
}
