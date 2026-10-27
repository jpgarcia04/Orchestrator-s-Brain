//! Respaldos consistentes, verificables y ensayables de una base cifrada.
//!
//! Comprueba el código de producción (`obrain_api::respaldo` y el binario
//! `obrain`) usando exclusivamente bases ficticias y archivos temporales.

use obrain_api::respaldo::{
    self, BASE_ARCHIVO, Error as RespaldoError, EtapaRespaldo, EtapaRestauracion, LLAVE_ARCHIVO,
    MANIFIESTO_ARCHIVO, Manifiesto, OpcionesRestauracion, RECIBO_TRANSICION_ARCHIVO,
    ReciboTransicion, comprobar_y_reparar_transicion, planificar, restaurar, restaurar_con_etapa,
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

fn construir_base_simple(db: &Path, num_aserciones: usize) -> String {
    if let Some(parent) = db.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut store = abrir_con(db, K).unwrap();
    Brain::initialize_store(&mut store).unwrap();
    let at = 1_700_000_000_000_i64;
    for i in 0..num_aserciones {
        let ent = store.create_entity(at + i as i64).unwrap();
        store
            .assert(NewAssertion::new(
                ent,
                "about",
                format!("tema-{i}"),
                Method::Deterministic,
                at + i as i64,
            ))
            .unwrap();
    }
    let brain = Brain::try_from_store(store).unwrap();
    let recovery = brain.recovery_code().unwrap().unwrap();
    brain.confirm_recovery_saved().unwrap();
    recovery
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

#[test]
fn restauracion_correcta_de_base_cifrada_con_salvaguarda_verificable() {
    let dir = CarpetaPrueba::nueva("restaurar-completa");
    let fx = construir_base_realista(&dir.path().join("fuente"));
    let respaldo_dir = dir.path().join("respaldo");
    let man = respaldo::crear(&fx.db, &respaldo_dir).unwrap();

    // Perfil activo inicial con 7 aserciones
    let db_activa = dir.path().join("perfil_activo").join("brain.obrain");
    let _clave_activa = construir_base_simple(&db_activa, 7);

    let opciones = OpcionesRestauracion {
        codigo_recuperacion: None,
        autorizar: true,
        pausar_reindexado: true,
        carpeta_salvaguarda: None,
    };
    let informe = restaurar(&respaldo_dir, &db_activa, opciones).unwrap();
    assert_eq!(
        informe.manifiesto.instantanea.entidades,
        man.instantanea.entidades
    );
    assert_eq!(
        informe.manifiesto.instantanea.aserciones,
        man.instantanea.aserciones
    );
    assert!(informe.reindexado_en_pausa);

    // 1. Salvaguarda existe y conserva el estado previo
    let salv_dir = informe.salvaguarda_creada.expect("debe crear salvaguarda");
    assert!(salv_dir.join(BASE_ARCHIVO).exists());
    assert!(salv_dir.join(LLAVE_ARCHIVO).exists());
    assert!(salv_dir.join("info.json").exists());

    {
        let store_salv = abrir_con(&salv_dir.join(BASE_ARCHIVO), K).unwrap();
        let brain_salv = Brain::try_from_store(store_salv).unwrap();
        let log_len = brain_salv.store().log_len().unwrap();
        assert!(log_len >= 7);
    }

    // 2. Base activa restaurada
    {
        let store_rest = abrir_con(&db_activa, K).unwrap();
        assert!(seguridad::reindexado_pausado(store_rest.conn()).unwrap());

        let brain_rest = Brain::try_from_store(store_rest).unwrap();

        // Conserva el PIN: 9999 falla
        assert!(brain_rest.security().unwrap().pin);
        assert!(brain_rest.unlock("9999").is_err());

        // Colecciones conservadas
        let colecciones = brain_rest.collections().unwrap();
        assert_eq!(colecciones.len(), 1);
        assert_eq!(colecciones[0].name, "Auditoria Sintetica");
        assert_eq!(colecciones[0].documents, 2);

        // Carpetas marcadas conservadas
        let marcadas = brain_rest.marked_folders().unwrap();
        assert_eq!(marcadas.len(), 2);

        // Bóveda protegida antes de desbloquear
        let ruta_boveda = std::path::absolute(&fx.doc_boveda)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let id_boveda = brain_rest.document_at(&ruta_boveda).unwrap().unwrap();
        assert!(brain_rest.preview(&id_boveda, None).unwrap().locked);

        // Desbloquear PIN y comprobar acceso a secreto
        assert!(brain_rest.unlock(fx.pin).is_ok());
        let prev = brain_rest.preview(&id_boveda, None).unwrap();
        assert!(!prev.locked);
        assert!(prev.text.as_deref().unwrap().contains(SECRETO_BOVEDA));

        // Paquetes y búsqueda FTS5
        let paquetes = brain_rest.packages().unwrap();
        assert_eq!(paquetes.len(), 1);
        let busq = brain_rest.search("arrendamiento", 10).unwrap();
        assert!(!busq.hits.is_empty());
    }
}

#[test]
fn restauracion_requiere_autorizacion_y_respeta_plan() {
    let dir = CarpetaPrueba::nueva("restaurar-autorizar");
    let fx = construir_base_realista(&dir.path().join("fuente"));
    let respaldo_dir = dir.path().join("respaldo");
    respaldo::crear(&fx.db, &respaldo_dir).unwrap();

    let db_activa = dir.path().join("activa").join("brain.obrain");
    construir_base_simple(&db_activa, 4);
    let hash_activa_antes = blake3::hash(&std::fs::read(&db_activa).unwrap())
        .to_hex()
        .to_string();

    // 1. Diagnóstico del plan
    let plan = planificar(&respaldo_dir, &db_activa, None).unwrap();
    assert!(plan.perfil_actual.existe);
    assert_eq!(plan.perfil_actual.entidades, 4);
    assert!(plan.llave_compatible_sistema);
    assert!(
        plan.advertencias
            .iter()
            .any(|a| a.contains("Se sustituirá el perfil existente"))
    );

    // 2. Intento de restaurar sin autorizar
    let opciones_sin_autorizar = OpcionesRestauracion {
        codigo_recuperacion: None,
        autorizar: false,
        pausar_reindexado: true,
        carpeta_salvaguarda: None,
    };
    let err = restaurar(&respaldo_dir, &db_activa, opciones_sin_autorizar).unwrap_err();
    assert!(matches!(err, RespaldoError::NoAutorizado));

    // Perfil activo intacto
    let hash_activa_despues = blake3::hash(&std::fs::read(&db_activa).unwrap())
        .to_hex()
        .to_string();
    assert_eq!(hash_activa_antes, hash_activa_despues);
    assert!(!db_activa.parent().unwrap().join("salvaguardas").exists());
}

#[test]
fn restauracion_rechaza_conexiones_abiertas() {
    let dir = CarpetaPrueba::nueva("restaurar-bloqueada");
    let fx = construir_base_realista(&dir.path().join("fuente"));
    let respaldo_dir = dir.path().join("respaldo");
    respaldo::crear(&fx.db, &respaldo_dir).unwrap();

    let db_activa = dir.path().join("activa").join("brain.obrain");
    construir_base_simple(&db_activa, 3);
    let hash_activa_antes = blake3::hash(&std::fs::read(&db_activa).unwrap())
        .to_hex()
        .to_string();

    // Abrir conexión sobre la base activa y retenerla
    let brain_bloqueador = Brain::open(&db_activa).unwrap();

    let opciones = OpcionesRestauracion {
        codigo_recuperacion: None,
        autorizar: true,
        pausar_reindexado: true,
        carpeta_salvaguarda: None,
    };
    let err = restaurar(&respaldo_dir, &db_activa, opciones.clone()).unwrap_err();
    assert!(
        matches!(err, RespaldoError::BaseEnUso(ref p) if p == &db_activa),
        "esperado BaseEnUso, obtenido: {err:?}"
    );

    // La base activa permanece intacta
    assert_eq!(
        hash_activa_antes,
        blake3::hash(&std::fs::read(&db_activa).unwrap())
            .to_hex()
            .to_string()
    );

    // Al cerrar la conexión activa, la restauración puede proceder
    drop(brain_bloqueador);

    let res = restaurar(&respaldo_dir, &db_activa, opciones);
    assert!(res.is_ok(), "tras cerrar conexion debe restaurar: {res:?}");
}

#[test]
fn restauracion_falla_con_respaldo_invalido_sin_tocar_perfil_activo() {
    let dir = CarpetaPrueba::nueva("restaurar-invalido");
    let fx = construir_base_realista(&dir.path().join("fuente"));
    let respaldo_dir = dir.path().join("respaldo");
    respaldo::crear(&fx.db, &respaldo_dir).unwrap();

    // Corromper la base del respaldo
    let db_en_respaldo = respaldo_dir.join(BASE_ARCHIVO);
    let mut bytes = std::fs::read(&db_en_respaldo).unwrap();
    bytes[100] ^= 0xFF;
    std::fs::write(&db_en_respaldo, bytes).unwrap();

    let db_activa = dir.path().join("activa").join("brain.obrain");
    construir_base_simple(&db_activa, 5);
    let hash_activa_antes = blake3::hash(&std::fs::read(&db_activa).unwrap())
        .to_hex()
        .to_string();

    let opciones = OpcionesRestauracion {
        codigo_recuperacion: None,
        autorizar: true,
        pausar_reindexado: true,
        carpeta_salvaguarda: None,
    };
    let err = restaurar(&respaldo_dir, &db_activa, opciones).unwrap_err();
    assert!(matches!(err, RespaldoError::HashNoCoincide { .. }));

    // El perfil activo no se ha alterado ni se han dejado salvaguardas o archivos residuales
    let hash_activa_despues = blake3::hash(&std::fs::read(&db_activa).unwrap())
        .to_hex()
        .to_string();
    assert_eq!(hash_activa_antes, hash_activa_despues);
    assert!(!db_activa.with_extension("obrain.anterior").exists());
}

#[test]
fn restauracion_con_clave_en_entorno_dpapi_distinto() {
    let dir = CarpetaPrueba::nueva("restaurar-dpapi-ajeno");
    let fx = construir_base_realista(&dir.path().join("fuente"));
    let respaldo_dir = dir.path().join("respaldo");
    respaldo::crear(&fx.db, &respaldo_dir).unwrap();

    // Simular que el respaldo proviene de otro equipo / usuario cuyo DPAPI no abre en esta sesión
    let llave_path = respaldo_dir.join(LLAVE_ARCHIVO);
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

    let man_path = respaldo_dir.join(MANIFIESTO_ARCHIVO);
    let mut man: Manifiesto =
        serde_json::from_str(&std::fs::read_to_string(&man_path).unwrap()).unwrap();
    man.llave.bytes = llave_mod.len() as u64;
    man.llave.blake3 = blake3::hash(llave_mod.as_bytes()).to_hex().to_string();
    std::fs::write(&man_path, serde_json::to_string_pretty(&man).unwrap()).unwrap();

    let db_activa = dir.path().join("activa").join("brain.obrain");
    construir_base_simple(&db_activa, 2);

    // Planificar sin clave indica que la llave no está disponible en este sistema
    let err_sin_clave = planificar(&respaldo_dir, &db_activa, None).unwrap_err();
    assert!(obrain_api::Error::from(err_sin_clave).is_key_unavailable());

    // Planificar con clave de recuperación diagnostica y reporta que requiere la clave
    let plan = planificar(&respaldo_dir, &db_activa, Some(&fx.clave_recuperacion)).unwrap();
    assert!(plan.requiere_clave_recuperacion);

    // Intentar restaurar sin clave falla
    let opciones_sin_clave = OpcionesRestauracion {
        codigo_recuperacion: None,
        autorizar: true,
        pausar_reindexado: true,
        carpeta_salvaguarda: None,
    };
    assert!(restaurar(&respaldo_dir, &db_activa, opciones_sin_clave).is_err());

    // Restaurar con clave de recuperación re-empaqueta la llave para el DPAPI local
    let opciones_con_clave = OpcionesRestauracion {
        codigo_recuperacion: Some(fx.clave_recuperacion.clone()),
        autorizar: true,
        pausar_reindexado: true,
        carpeta_salvaguarda: None,
    };
    let informe = restaurar(&respaldo_dir, &db_activa, opciones_con_clave).unwrap();
    assert!(informe.recuperada_con_clave);

    // Ahora la base activa abre nativamente en este equipo con Brain::open (DPAPI propio)
    let brain_activa = Brain::open(&db_activa).unwrap();
    assert!(matches!(
        brain_activa.recovery_code(),
        Err(Error::Bloqueado)
    ));
    brain_activa.unlock(fx.pin).unwrap();
    assert_eq!(
        brain_activa.recovery_code().unwrap().as_deref(),
        Some(fx.clave_recuperacion.as_str())
    );
}

#[test]
fn restauracion_rollback_ante_fallos_en_distintas_etapas() {
    let dir = CarpetaPrueba::nueva("restaurar-rollback");
    let fx = construir_base_realista(&dir.path().join("fuente"));
    let respaldo_dir = dir.path().join("respaldo");
    respaldo::crear(&fx.db, &respaldo_dir).unwrap();

    // Etapa 1: Fallo tras crear la salvaguarda (antes de tocar activos)
    {
        let db_activa = dir.path().join("activa1").join("brain.obrain");
        construir_base_simple(&db_activa, 5);
        let hash_activa = blake3::hash(&std::fs::read(&db_activa).unwrap())
            .to_hex()
            .to_string();

        let opciones = OpcionesRestauracion {
            codigo_recuperacion: None,
            autorizar: true,
            pausar_reindexado: true,
            carpeta_salvaguarda: None,
        };
        let err = restaurar_con_etapa(&respaldo_dir, &db_activa, opciones, |etapa| {
            if matches!(etapa, EtapaRestauracion::TrasCrearSalvaguarda) {
                Err(RespaldoError::DestinoInvalido(
                    "error simulado tras crear salvaguarda".into(),
                ))
            } else {
                Ok(())
            }
        })
        .unwrap_err();

        assert!(matches!(err, RespaldoError::DestinoInvalido(_)));
        assert_eq!(
            hash_activa,
            blake3::hash(&std::fs::read(&db_activa).unwrap())
                .to_hex()
                .to_string()
        );
    }

    // Etapa 2: Fallo tras desplazar activa a .anterior (rollback restablece .anterior)
    {
        let db_activa = dir.path().join("activa2").join("brain.obrain");
        construir_base_simple(&db_activa, 8);
        let hash_activa = blake3::hash(&std::fs::read(&db_activa).unwrap())
            .to_hex()
            .to_string();

        let opciones = OpcionesRestauracion {
            codigo_recuperacion: None,
            autorizar: true,
            pausar_reindexado: true,
            carpeta_salvaguarda: None,
        };
        let err = restaurar_con_etapa(&respaldo_dir, &db_activa, opciones, |etapa| {
            if matches!(etapa, EtapaRestauracion::TrasDesplazarActiva) {
                Err(RespaldoError::DestinoInvalido(
                    "error simulado tras mover a .anterior".into(),
                ))
            } else {
                Ok(())
            }
        })
        .unwrap_err();

        assert!(
            matches!(err, RespaldoError::RestauracionFallidaYRecuperada(_)),
            "esperado RestauracionFallidaYRecuperada, obtenido: {err:?}"
        );
        // Base activa restaurada a su ubicación original
        assert_eq!(
            hash_activa,
            blake3::hash(&std::fs::read(&db_activa).unwrap())
                .to_hex()
                .to_string()
        );
        assert!(!db_activa.with_extension("obrain.anterior").exists());

        // Abre normalmente con todas sus aserciones originales
        let store = abrir_con(&db_activa, K).unwrap();
        let brain = Brain::try_from_store(store).unwrap();
        assert!(brain.store().log_len().unwrap() >= 8);
    }

    // Etapa 3: Fallo antes de validar final (archivos ya sustituidos -> rollback recupera de salvaguarda/.anterior)
    {
        let db_activa = dir.path().join("activa3").join("brain.obrain");
        construir_base_simple(&db_activa, 6);
        let hash_activa = blake3::hash(&std::fs::read(&db_activa).unwrap())
            .to_hex()
            .to_string();

        let opciones = OpcionesRestauracion {
            codigo_recuperacion: None,
            autorizar: true,
            pausar_reindexado: true,
            carpeta_salvaguarda: None,
        };
        let err = restaurar_con_etapa(&respaldo_dir, &db_activa, opciones, |etapa| {
            if matches!(etapa, EtapaRestauracion::AntesDeValidarFinal) {
                Err(RespaldoError::DestinoInvalido(
                    "error simulado antes de validacion".into(),
                ))
            } else {
                Ok(())
            }
        })
        .unwrap_err();

        assert!(
            matches!(err, RespaldoError::RestauracionFallidaYRecuperada(_)),
            "esperado RestauracionFallidaYRecuperada, obtenido: {err:?}"
        );
        assert_eq!(
            hash_activa,
            blake3::hash(&std::fs::read(&db_activa).unwrap())
                .to_hex()
                .to_string()
        );
        assert!(!db_activa.with_extension("obrain.anterior").exists());

        let store = abrir_con(&db_activa, K).unwrap();
        let brain = Brain::try_from_store(store).unwrap();
        assert!(brain.store().log_len().unwrap() >= 6);
    }
}

#[test]
fn restauracion_repara_transicion_interrumpida() {
    let dir = CarpetaPrueba::nueva("restaurar-interrumpida");
    let db_activa = dir.path().join("activa").join("brain.obrain");
    let parent = db_activa.parent().unwrap();
    construir_base_simple(&db_activa, 5);
    let hash_activa_original = blake3::hash(&std::fs::read(&db_activa).unwrap())
        .to_hex()
        .to_string();
    let llave_activa = obrain_keys::key_path(&db_activa);
    let hash_llave_original = blake3::hash(&std::fs::read(&llave_activa).unwrap())
        .to_hex()
        .to_string();

    // 1. Simular interrupción durante sustitución:
    // La base original se desplazó a .anterior, se escribió el recibo con estado 'reemplazando'.
    let db_anterior = db_activa.with_extension("obrain.anterior");
    let llave_anterior = llave_activa.with_extension("llave.anterior");
    std::fs::rename(&db_activa, &db_anterior).unwrap();
    std::fs::rename(&llave_activa, &llave_anterior).unwrap();

    let staging_fake = dir.path().join("staging-fake");
    std::fs::create_dir_all(&staging_fake).unwrap();

    let recibo = ReciboTransicion {
        version: 1,
        creado_en_ms: 1000,
        salvaguarda_dir: None,
        staging_dir: staging_fake.clone(),
        target_db: db_activa.clone(),
        target_llave: llave_activa.clone(),
        staged_db_hash: "hash_staged_ficticio".into(),
        staged_llave_hash: "hash_llave_staged_ficticio".into(),
        target_db_hash_anterior: Some(hash_activa_original.clone()),
        target_llave_hash_anterior: Some(hash_llave_original.clone()),
        estado: "reemplazando".into(),
    };
    let recibo_path = parent.join(RECIBO_TRANSICION_ARCHIVO);
    std::fs::write(&recibo_path, serde_json::to_string_pretty(&recibo).unwrap()).unwrap();

    // Comprobar y reparar detecta la interrupción y recupera el perfil original
    let reparado = comprobar_y_reparar_transicion(&db_activa).unwrap();
    assert!(reparado.is_some());
    let msg = reparado.unwrap();
    assert!(
        msg.contains("se recuperó el estado original"),
        "mensaje: {msg}"
    );

    // El recibo y .anterior desaparecen, y la base activa vuelve a abrir
    assert!(!recibo_path.exists());
    assert!(!db_anterior.exists());
    assert!(db_activa.exists());
    assert_eq!(
        hash_activa_original,
        blake3::hash(&std::fs::read(&db_activa).unwrap())
            .to_hex()
            .to_string()
    );

    let brain = Brain::open(&db_activa).unwrap();
    assert!(brain.store().log_len().unwrap() >= 5);
}

#[test]
fn apertura_repara_transicion_interrumpida_directamente() {
    let dir = CarpetaPrueba::nueva("apertura-repara-transicion");
    let db_activa = dir.path().join("activa").join("brain.obrain");
    let parent = db_activa.parent().unwrap();
    construir_base_simple(&db_activa, 4);
    let hash_activa_original = blake3::hash(&std::fs::read(&db_activa).unwrap())
        .to_hex()
        .to_string();
    let llave_activa = obrain_keys::key_path(&db_activa);
    let hash_llave_original = blake3::hash(&std::fs::read(&llave_activa).unwrap())
        .to_hex()
        .to_string();

    // Simular corte eléctrico en pleno reemplazo:
    let db_anterior = db_activa.with_extension("obrain.anterior");
    let llave_anterior = llave_activa.with_extension("llave.anterior");
    std::fs::rename(&db_activa, &db_anterior).unwrap();
    std::fs::rename(&llave_activa, &llave_anterior).unwrap();

    let staging_fake = dir.path().join("staging-apertura-fake");
    std::fs::create_dir_all(&staging_fake).unwrap();

    let recibo = ReciboTransicion {
        version: 1,
        creado_en_ms: 2000,
        salvaguarda_dir: None,
        staging_dir: staging_fake.clone(),
        target_db: db_activa.clone(),
        target_llave: llave_activa.clone(),
        staged_db_hash: "hash_staged_ficticio".into(),
        staged_llave_hash: "hash_llave_staged_ficticio".into(),
        target_db_hash_anterior: Some(hash_activa_original.clone()),
        target_llave_hash_anterior: Some(hash_llave_original.clone()),
        estado: "reemplazando".into(),
    };
    let recibo_path = parent.join(RECIBO_TRANSICION_ARCHIVO);
    std::fs::write(&recibo_path, serde_json::to_string_pretty(&recibo).unwrap()).unwrap();

    // Brain::open directamente sin llamar antes a ninguna función manual de reparación
    let brain = Brain::open(&db_activa).unwrap();
    assert!(brain.store().log_len().unwrap() >= 4);

    // Evidencias de recuperación satisfactoria
    assert!(!recibo_path.exists());
    assert!(!db_anterior.exists());
    assert!(db_activa.exists());
}

#[test]
fn restauracion_transicion_ambigua_se_detiene_y_preserva_evidencias() {
    let dir = CarpetaPrueba::nueva("transicion-ambigua");
    let db_activa = dir.path().join("activa").join("brain.obrain");
    let parent = db_activa.parent().unwrap();
    std::fs::create_dir_all(parent).unwrap();

    std::fs::write(&db_activa, b"contenido-desconocido-no-respaldo").unwrap();
    let hash_desconocido = blake3::hash(b"contenido-desconocido-no-respaldo")
        .to_hex()
        .to_string();

    let staging_fake = dir.path().join("staging-fake");
    std::fs::create_dir_all(&staging_fake).unwrap();

    let recibo = ReciboTransicion {
        version: 1,
        creado_en_ms: 1000,
        salvaguarda_dir: None,
        staging_dir: staging_fake.clone(),
        target_db: db_activa.clone(),
        target_llave: obrain_keys::key_path(&db_activa),
        staged_db_hash: "hash_staged_ficticio".into(),
        staged_llave_hash: "hash_llave_staged_ficticio".into(),
        target_db_hash_anterior: Some("hash_anterior_otro".into()),
        target_llave_hash_anterior: None,
        estado: "reemplazando".into(),
    };
    let recibo_path = parent.join(RECIBO_TRANSICION_ARCHIVO);
    std::fs::write(&recibo_path, serde_json::to_string_pretty(&recibo).unwrap()).unwrap();

    // Ante ambigüedad, comprobar_y_reparar_transicion debe fallar
    let res = comprobar_y_reparar_transicion(&db_activa);
    assert!(res.is_err(), "debe fallar ante estado ambiguo");
    match res.unwrap_err() {
        respaldo::Error::TransicionAmbigua(p) => {
            assert_eq!(p, recibo_path);
        }
        other => panic!("error inesperado: {other:?}"),
    }

    // Comprobar que NO se modificó ni borró ningún archivo y se preservaron las evidencias
    assert!(recibo_path.exists(), "el recibo debe conservarse intacto");
    assert!(
        db_activa.exists(),
        "el archivo en db_activa no debe borrarse"
    );
    assert_eq!(
        hash_desconocido,
        blake3::hash(&std::fs::read(&db_activa).unwrap())
            .to_hex()
            .to_string()
    );
    assert!(staging_fake.exists(), "el staging debe conservarse");

    // Brain::open tampoco debe destruir las evidencias y debe fallar
    let open_res = Brain::open(&db_activa);
    assert!(
        open_res.is_err(),
        "Brain::open debe detenerse ante estado ambiguo"
    );
    assert!(
        recibo_path.exists(),
        "evidencia de recibo debe seguir preservada"
    );
    assert!(
        db_activa.exists(),
        "evidencia de base debe seguir preservada"
    );
}

#[test]
fn restauracion_fallo_en_propio_rollback_preserva_evidencias_y_recibo() {
    let dir = CarpetaPrueba::nueva("rollback-fallido-preserva");
    let fx_origen = dir.path().join("origen").join("brain.obrain");
    std::fs::create_dir_all(fx_origen.parent().unwrap()).unwrap();
    construir_base_simple(&fx_origen, 5);
    let respaldo_dir = dir.path().join("respaldo");
    respaldo::crear(&fx_origen, &respaldo_dir).unwrap();

    let db_activa = dir.path().join("activa").join("brain.obrain");
    std::fs::create_dir_all(db_activa.parent().unwrap()).unwrap();
    construir_base_simple(&db_activa, 7);
    let db_anterior = db_activa.with_extension("obrain.anterior");
    let recibo_path = db_activa.parent().unwrap().join(RECIBO_TRANSICION_ARCHIVO);

    let opciones = OpcionesRestauracion {
        codigo_recuperacion: None,
        autorizar: true,
        pausar_reindexado: true,
        carpeta_salvaguarda: None,
    };

    // Provocar fallo de E/S en pleno rollback:
    // Al alcanzar TrasDesplazarActiva, la base activa ya se renombró a .anterior.
    // Creamos un directorio no vacío en db_activa para que el rename de rollback falle con error de E/S.
    let err = restaurar_con_etapa(&respaldo_dir, &db_activa, opciones, |etapa| {
        if matches!(etapa, EtapaRestauracion::TrasDesplazarActiva) {
            std::fs::create_dir_all(&db_activa).unwrap();
            std::fs::write(db_activa.join("bloqueo.bin"), b"bloqueo").unwrap();
            Err(RespaldoError::DestinoInvalido("error inyectado".into()))
        } else {
            Ok(())
        }
    })
    .unwrap_err();

    // El error debe ser RestauracionCritica indicando la situación
    assert!(
        matches!(err, RespaldoError::RestauracionCritica(..)),
        "esperado RestauracionCritica, obtenido: {err:?}"
    );

    // Evidencias imprescindibles deben estar PRESERVADAS sin borrado destructivo
    assert!(
        recibo_path.exists(),
        "el recibo de transición debe conservarse para intervención manual"
    );
    assert!(
        db_anterior.exists(),
        "la base .anterior no debe borrarse si el rollback falló"
    );

    // Limpiamos el bloqueo inyectado para comprobar que la transición se puede curar
    let _ = std::fs::remove_dir_all(&db_activa);
    let reparado = comprobar_y_reparar_transicion(&db_activa).unwrap();
    assert!(reparado.is_some());
    assert!(db_activa.exists());
    assert!(!recibo_path.exists());
    assert!(!db_anterior.exists());
}

#[test]
fn transicion_con_base_anterior_valida_pero_llave_ausente_o_invalida() {
    let dir = CarpetaPrueba::nueva("llave-anterior-ausente-o-invalida");
    let db_activa = dir.path().join("activa").join("brain.obrain");
    let parent = db_activa.parent().unwrap();
    construir_base_simple(&db_activa, 4);

    let hash_db_original = blake3::hash(&std::fs::read(&db_activa).unwrap())
        .to_hex()
        .to_string();
    let llave_activa = obrain_keys::key_path(&db_activa);
    let hash_llave_original = blake3::hash(&std::fs::read(&llave_activa).unwrap())
        .to_hex()
        .to_string();

    let db_anterior = db_activa.with_extension("obrain.anterior");
    let llave_anterior = llave_activa.with_extension("llave.anterior");
    std::fs::rename(&db_activa, &db_anterior).unwrap();
    let _ = std::fs::remove_file(&llave_activa);

    let staging_fake = dir.path().join("staging-fake");
    std::fs::create_dir_all(&staging_fake).unwrap();

    let recibo_path = parent.join(RECIBO_TRANSICION_ARCHIVO);

    // 1. Caso A: Base anterior existe y es válida, pero llave_anterior NO existe y no hay salvaguarda
    let recibo_sin_llave = ReciboTransicion {
        version: 1,
        creado_en_ms: 1000,
        salvaguarda_dir: None,
        staging_dir: staging_fake.clone(),
        target_db: db_activa.clone(),
        target_llave: llave_activa.clone(),
        staged_db_hash: "staged_hash".into(),
        staged_llave_hash: "staged_llave".into(),
        target_db_hash_anterior: Some(hash_db_original),
        target_llave_hash_anterior: Some(hash_llave_original),
        estado: "reemplazando".into(),
    };
    std::fs::write(
        &recibo_path,
        serde_json::to_string_pretty(&recibo_sin_llave).unwrap(),
    )
    .unwrap();

    // Debe detenerse por ambigüedad porque no puede restaurar una base cifrada sin su llave
    let res_a = comprobar_y_reparar_transicion(&db_activa);
    assert!(res_a.is_err(), "debe fallar ante llave anterior ausente");
    assert!(matches!(
        res_a.unwrap_err(),
        RespaldoError::TransicionAmbigua(_)
    ));
    assert!(recibo_path.exists(), "recibo preservado");
    assert!(db_anterior.exists(), "db_anterior preservada");
    assert!(
        !db_activa.exists(),
        "no debe crearse base sin llave en destino"
    );

    // 2. Caso B: llave_anterior existe pero su hash está corrupto / no corresponde
    std::fs::write(&llave_anterior, b"llave-corrupta-invalida").unwrap();
    let res_b = comprobar_y_reparar_transicion(&db_activa);
    assert!(res_b.is_err(), "debe fallar ante llave anterior corrupta");
    assert!(matches!(
        res_b.unwrap_err(),
        RespaldoError::TransicionAmbigua(_)
    ));
    assert!(recibo_path.exists());
    assert!(db_anterior.exists());
    assert!(llave_anterior.exists());

    // 3. Caso C: llave_anterior está corrupta, pero la salvaguarda contiene base Y llave válidas
    let salv_dir = dir.path().join("salvaguarda_valida");
    std::fs::create_dir_all(&salv_dir).unwrap();
    std::fs::copy(&db_anterior, salv_dir.join(BASE_ARCHIVO)).unwrap();
    let mut recibo_con_salv = recibo_sin_llave.clone();
    recibo_con_salv.salvaguarda_dir = Some(salv_dir.clone());
    let salv_db = salv_dir.join(BASE_ARCHIVO);
    let salv_ll = salv_dir.join(LLAVE_ARCHIVO);
    let h_salv_db = blake3::hash(&std::fs::read(&salv_db).unwrap())
        .to_hex()
        .to_string();
    let h_salv_ll = blake3::hash(b"llave-salvaguarda-valida")
        .to_hex()
        .to_string();
    std::fs::write(&salv_ll, b"llave-salvaguarda-valida").unwrap();
    recibo_con_salv.target_db_hash_anterior = Some(h_salv_db);
    recibo_con_salv.target_llave_hash_anterior = Some(h_salv_ll.clone());
    std::fs::write(
        &recibo_path,
        serde_json::to_string_pretty(&recibo_con_salv).unwrap(),
    )
    .unwrap();

    // Ahora la curación ignora la .anterior dañada y recupera exitosamente desde la salvaguarda
    let res_c = comprobar_y_reparar_transicion(&db_activa).unwrap();
    assert!(res_c.is_some());
    let msg = res_c.unwrap();
    assert!(msg.contains("desde la salvaguarda"), "mensaje: {msg}");
    assert!(db_activa.exists());
    assert!(llave_activa.exists());
    assert_eq!(
        h_salv_ll,
        blake3::hash(&std::fs::read(&llave_activa).unwrap())
            .to_hex()
            .to_string()
    );
    assert!(!recibo_path.exists());
    assert!(!db_anterior.exists());
    assert!(!llave_anterior.exists());
}

#[test]
fn restauracion_de_copia_antigua_frente_a_mas_reciente() {
    let dir = CarpetaPrueba::nueva("restaurar-antigua");
    let fx_antigua = dir.path().join("respaldo_antiguo").join("brain.obrain");
    std::fs::create_dir_all(fx_antigua.parent().unwrap()).unwrap();
    construir_base_simple(&fx_antigua, 3);
    let carpeta_respaldo = dir.path().join("respaldo_empaquetado");
    respaldo::crear(&fx_antigua, &carpeta_respaldo).unwrap();

    // Perfil activo más reciente con 15 aserciones
    let db_activa = dir.path().join("activa").join("brain.obrain");
    construir_base_simple(&db_activa, 15);

    // El planificador advierte que la base activa actual contiene más entidades/aserciones
    let plan = planificar(&carpeta_respaldo, &db_activa, None).unwrap();
    assert!(
        plan.advertencias
            .iter()
            .any(|a| a.contains("El perfil actual contiene más entidades o aserciones")),
        "advertencias: {:?}",
        plan.advertencias
    );

    // Con autorización explícita, sustituye pero guarda salvaguarda con las 15 entidades
    let opciones = OpcionesRestauracion {
        codigo_recuperacion: None,
        autorizar: true,
        pausar_reindexado: true,
        carpeta_salvaguarda: None,
    };
    let informe = restaurar(&carpeta_respaldo, &db_activa, opciones).unwrap();
    assert_eq!(informe.manifiesto.instantanea.entidades, 3);

    let salv = informe.salvaguarda_creada.unwrap();
    let store_salv = abrir_con(&salv.join(BASE_ARCHIVO), K).unwrap();
    let brain_salv = Brain::try_from_store(store_salv).unwrap();
    assert!(brain_salv.store().log_len().unwrap() >= 15);
}

#[test]
fn restauracion_pausa_reindexado_y_bibliotecas_ausentes() {
    let dir = CarpetaPrueba::nueva("restaurar-pausa");
    let fx_db = dir.path().join("fuente").join("brain.obrain");
    std::fs::create_dir_all(fx_db.parent().unwrap()).unwrap();

    let bib_inexistente = dir.path().join("carpeta_inexistente_remota");
    {
        let mut store = abrir_con(&fx_db, K).unwrap();
        Brain::initialize_store(&mut store).unwrap();
        let at = obrain_scan::now_ms();
        store
            .transaction(|s| obrain_scan::register_library(s, &bib_inexistente, at))
            .unwrap();
    }

    let carpeta_respaldo = dir.path().join("respaldo");
    respaldo::crear(&fx_db, &carpeta_respaldo).unwrap();

    let db_activa = dir.path().join("activa").join("brain.obrain");
    construir_base_simple(&db_activa, 2);

    let opciones = OpcionesRestauracion {
        codigo_recuperacion: None,
        autorizar: true,
        pausar_reindexado: true,
        carpeta_salvaguarda: None,
    };
    let informe = restaurar(&carpeta_respaldo, &db_activa, opciones).unwrap();
    assert!(informe.reindexado_en_pausa);
    assert_eq!(informe.bibliotecas_ausentes.len(), 1);

    // 1. Al abrir la base, la reindexación está en pausa y devuelve aviso descriptivo
    let mut store = abrir_con(&db_activa, K).unwrap();
    assert!(seguridad::reindexado_pausado(store.conn()).unwrap());
    let motivo = seguridad::motivo_reindexado_pausado(store.conn()).unwrap();
    assert_eq!(motivo.as_deref(), Some("restauración de respaldo reciente"));

    let rep = indexar::reindex(&mut store, None).unwrap();
    assert_eq!(
        rep.paused.as_deref(),
        Some("restauración de respaldo reciente")
    );
    assert!(
        rep.errors.is_empty(),
        "una pausa preventiva intencionada no debe reportarse como error de lectura: {:?}",
        rep.errors
    );
    assert!(
        rep.missing.is_empty(),
        "mientras está en pausa no debe reportar bibliotecas como ausentes: {:?}",
        rep.missing
    );

    // 2. Al reanudar explícitamente, se sincroniza normalmente sin corromper el índice
    seguridad::reanudar_reindexado(store.conn()).unwrap();
    assert!(!seguridad::reindexado_pausado(store.conn()).unwrap());

    // La reindexación ahora se ejecuta, señala la raíz realmente ausente y no purga documentos
    let rep2 = indexar::reindex(&mut store, None).unwrap();
    assert!(rep2.paused.is_none());
    assert_eq!(rep2.missing.len(), 1);
    assert_eq!(rep2.trashed, 0);
}

#[test]
fn cli_obrain_restaurar_plan_y_ejecucion_con_confirmacion() {
    let dir = CarpetaPrueba::nueva("cli-restaurar");
    let fx = construir_base_realista(&dir.path().join("fuente"));
    let bin = env!("CARGO_BIN_EXE_obrain");

    let respaldo_dir = dir.path().join("respaldo");
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

    let db_activa = dir.path().join("activa").join("brain.obrain");
    construir_base_simple(&db_activa, 4);
    let hash_activa_antes = blake3::hash(&std::fs::read(&db_activa).unwrap())
        .to_hex()
        .to_string();

    // 1. obrain restaurar sin --confirmar muestra diagnóstico y plan pero no toca la base
    let out_plan = Command::new(bin)
        .args([
            "--db",
            db_activa.to_str().unwrap(),
            "restaurar",
            respaldo_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out_plan.status.success());
    let txt_plan = String::from_utf8_lossy(&out_plan.stdout);
    assert!(txt_plan.to_lowercase().contains("plan de restauración"));
    assert!(txt_plan.contains("Para autorizar la sustitución del perfil activo, ejecuta"));
    assert!(txt_plan.contains("--confirmar"));

    // Base activa no modificada
    assert_eq!(
        hash_activa_antes,
        blake3::hash(&std::fs::read(&db_activa).unwrap())
            .to_hex()
            .to_string()
    );

    // 2. obrain restaurar con --confirmar ejecuta la sustitución completa
    let out_rest = Command::new(bin)
        .args([
            "--db",
            db_activa.to_str().unwrap(),
            "restaurar",
            respaldo_dir.to_str().unwrap(),
            "--confirmar",
        ])
        .output()
        .unwrap();
    assert!(
        out_rest.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out_rest.stderr)
    );
    let txt_rest = String::from_utf8_lossy(&out_rest.stdout);
    assert!(txt_rest.contains("perfil restaurado con éxito"));
    assert!(txt_rest.contains("salvaguarda previa"));
    assert!(txt_rest.contains("reindexación           suspendida temporalmente"));

    // 3. obrain bibliotecas muestra el aviso de reindexación en pausa
    let out_bibs = Command::new(bin)
        .args(["--db", db_activa.to_str().unwrap(), "bibliotecas"])
        .output()
        .unwrap();
    assert!(out_bibs.status.success());
    let txt_bibs = String::from_utf8_lossy(&out_bibs.stdout);
    assert!(txt_bibs.contains("Reindexación automática en pausa"));
    assert!(!txt_bibs.contains("todavía no hay bibliotecas"));

    // 3b. obrain indexar durante la pausa explica la suspensión sin decir que no hay bibliotecas
    let out_idx_pausa = Command::new(bin)
        .args([
            "--db",
            db_activa.to_str().unwrap(),
            "indexar",
            "--sin-modelo",
        ])
        .output()
        .unwrap();
    assert!(out_idx_pausa.status.success());
    let txt_idx_pausa = String::from_utf8_lossy(&out_idx_pausa.stdout);
    assert!(
        txt_idx_pausa.contains("Reindexación suspendida"),
        "salida inesperada en pausa: {txt_idx_pausa}"
    );
    assert!(
        !txt_idx_pausa.contains("todavía no hay bibliotecas"),
        "no debe afirmar que faltan bibliotecas cuando está en pausa: {txt_idx_pausa}"
    );
    assert!(
        !txt_idx_pausa.contains("recorrido aplazado"),
        "no debe presentar la pausa como error de recorrido: {txt_idx_pausa}"
    );

    // 4. obrain bibliotecas --confirmar reanuda la reindexación
    let out_reanudar = Command::new(bin)
        .args([
            "--db",
            db_activa.to_str().unwrap(),
            "bibliotecas",
            "--confirmar",
        ])
        .output()
        .unwrap();
    assert!(out_reanudar.status.success());
    let txt_reanudar = String::from_utf8_lossy(&out_reanudar.stdout);
    assert!(txt_reanudar.contains("reindexación reanudada"));
}

#[test]
fn salvaguarda_previa_se_reconoce_y_explica_sin_modificar_archivos() {
    let dir = CarpetaPrueba::nueva("salvaguarda-como-respaldo");
    let fx_origen = dir.path().join("origen").join("brain.obrain");
    std::fs::create_dir_all(fx_origen.parent().unwrap()).unwrap();
    construir_base_simple(&fx_origen, 4);
    let respaldo_dir = dir.path().join("respaldo-exportado");
    respaldo::crear(&fx_origen, &respaldo_dir).unwrap();

    let db_activa = dir.path().join("activa").join("brain.obrain");
    construir_base_simple(&db_activa, 6);

    let informe = restaurar(
        &respaldo_dir,
        &db_activa,
        OpcionesRestauracion {
            codigo_recuperacion: None,
            autorizar: true,
            pausar_reindexado: true,
            carpeta_salvaguarda: None,
        },
    )
    .unwrap();
    let salv_dir = informe.salvaguarda_creada.expect("debe crear salvaguarda");
    let salv_padre = salv_dir.parent().unwrap().to_path_buf();

    // Capturar estado exacto de los archivos de la salvaguarda antes de probar.
    let instantanea_archivos = |carpeta: &std::path::Path| -> Vec<(String, String)> {
        let mut pares: Vec<(String, String)> = std::fs::read_dir(carpeta)
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                let nombre = e.file_name().to_string_lossy().to_string();
                let hash = blake3::hash(&std::fs::read(e.path()).unwrap())
                    .to_hex()
                    .to_string();
                (nombre, hash)
            })
            .collect();
        pares.sort();
        pares
    };
    let antes = instantanea_archivos(&salv_dir);

    // 1. Verificar, ensayar, planificar y restaurar reconocen la salvaguarda con mensaje claro.
    let err_ver = respaldo::verificar(&salv_dir).unwrap_err();
    assert!(
        matches!(err_ver, RespaldoError::EsSalvaguarda(_)),
        "esperado EsSalvaguarda, obtenido: {err_ver:?}"
    );
    let msg = err_ver.to_string();
    assert!(msg.contains("salvaguarda"), "mensaje: {msg}");
    assert!(msg.contains("info.json"), "mensaje: {msg}");
    assert!(msg.contains("manifiesto.json"), "mensaje: {msg}");
    assert!(
        !msg.contains("archivo auxiliar o desconocido"),
        "mensaje: {msg}"
    );

    assert!(matches!(
        respaldo::ensayar(&salv_dir, None, None, None),
        Err(RespaldoError::EsSalvaguarda(_))
    ));
    assert!(matches!(
        planificar(&salv_dir, &db_activa, None),
        Err(RespaldoError::EsSalvaguarda(_))
    ));
    assert!(matches!(
        restaurar(
            &salv_dir,
            &db_activa,
            OpcionesRestauracion {
                autorizar: true,
                ..Default::default()
            }
        ),
        Err(RespaldoError::EsSalvaguarda(_))
    ));

    // 2. Seleccionar la carpeta contenedora `salvaguardas` también recibe explicación clara.
    assert!(matches!(
        respaldo::verificar(&salv_padre),
        Err(RespaldoError::EsSalvaguarda(_))
    ));

    // 3. La CLI también ofrece la explicación clara sin error técnico genérico.
    let bin = env!("CARGO_BIN_EXE_obrain");
    let out_cli = Command::new(bin)
        .args(["respaldo", salv_dir.to_str().unwrap(), "--verificar"])
        .output()
        .unwrap();
    assert!(!out_cli.status.success());
    let stderr_cli = String::from_utf8_lossy(&out_cli.stderr);
    assert!(stderr_cli.contains("salvaguarda"), "stderr: {stderr_cli}");
    assert!(
        !stderr_cli.contains("archivo auxiliar o desconocido"),
        "stderr: {stderr_cli}"
    );

    // 4. Ningún archivo de la salvaguarda fue convertido ni alterado.
    assert_eq!(antes, instantanea_archivos(&salv_dir));
    assert!(!salv_dir.join(MANIFIESTO_ARCHIVO).exists());

    // 5. Un respaldo convencional contaminado con un `info.json` extra sigue rechazándose como ArchivoInesperado.
    let respaldo_contaminado = dir.path().join("respaldo-con-extra");
    std::fs::create_dir_all(&respaldo_contaminado).unwrap();
    for f in [BASE_ARCHIVO, LLAVE_ARCHIVO, MANIFIESTO_ARCHIVO] {
        std::fs::copy(respaldo_dir.join(f), respaldo_contaminado.join(f)).unwrap();
    }
    std::fs::write(respaldo_contaminado.join("info.json"), b"{}").unwrap();
    assert!(matches!(
        respaldo::verificar(&respaldo_contaminado),
        Err(RespaldoError::ArchivoInesperado(_))
    ));
}
