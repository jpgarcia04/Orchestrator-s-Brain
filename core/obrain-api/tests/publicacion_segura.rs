//! Regresiones de publicación: sólo archivos inventados y bases temporales.

use obrain_api::{Brain, Error, indexar};
use obrain_core::{EntityId, Method, Store, TimePoint, Value};
use obrain_extract::{ExtractReport, extract_all_with, load_files};
use obrain_scan::{IngestReport, Scanner, ingest};
use std::path::{Path, PathBuf};

struct Fixture {
    root: PathBuf,
    db: PathBuf,
    file: PathBuf,
    writer: Store,
}

impl Fixture {
    fn new(label: &str, text: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "obrain-publicacion-{label}-{}-{}",
            std::process::id(),
            obrain_scan::now_ms()
        ));
        let library = root.join("Biblioteca");
        std::fs::create_dir_all(&library).unwrap();
        let file = library.join("notas.txt");
        std::fs::write(&file, text).unwrap();
        let db = root.join("brain.db");
        let writer = Store::open(&db).unwrap();
        Self {
            root,
            db,
            file,
            writer,
        }
    }

    fn library(&self) -> &Path {
        self.file.parent().unwrap()
    }

    fn scan_only(&mut self) -> EntityId {
        let scanner = Scanner::new();
        let (mut candidates, mut scan) = scanner.scan(self.library());
        scanner.hash_all(&mut candidates, &mut scan);
        assert!(scan.complete && scan.errors.is_empty());
        let at = obrain_scan::now_ms();
        let mut ingested = IngestReport::default();
        self.writer
            .transaction(|s| ingest(s, &candidates, at, &mut ingested))
            .unwrap();
        self.writer
            .find_subject_by_value(
                "path_observed_at",
                &self.file.to_string_lossy(),
                TimePoint::at(obrain_scan::now_ms()),
            )
            .unwrap()
            .unwrap()
    }

    fn stage_until_extracted(&mut self) -> EntityId {
        let id = self.scan_only();
        let files = load_files(&self.writer, TimePoint::at(obrain_scan::now_ms())).unwrap();
        let mut report = ExtractReport::default();
        self.writer
            .transaction(|s| extract_all_with(s, &files, obrain_scan::now_ms(), &mut report, None))
            .unwrap();
        assert_eq!(report.extracted, 1);
        id
    }

    fn reader(&self) -> Brain {
        Brain::from_store(Store::open(&self.db).unwrap())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

const SECRET: &str = "sk-ABCDEFGHIJKLMNOPQRSTUVWXYZ12345678901234567890";

#[test]
fn documento_nuevo_no_se_publica_entre_extraccion_y_clasificacion() {
    let mut f = Fixture::new("nuevo", &format!("Clave ficticia: {SECRET}. Fin."));
    let id = f.stage_until_extracted();
    assert!(
        !obrain_guard::gate::check(
            &f.writer,
            &id,
            obrain_guard::gate::Capability::Embedder,
            obrain_guard::gate::Runs::Local,
        )
        .unwrap()
        .allowed()
    );
    assert!(
        !obrain_guard::gate::embeddable(f.writer.conn())
            .unwrap()
            .contains(&id)
    );
    let reader = f.reader();
    assert!(reader.search(SECRET, 10).unwrap().hits.is_empty());
    assert!(
        reader
            .search("El archivo con la clave ficticia", 10)
            .unwrap()
            .hits
            .is_empty()
    );
    assert!(matches!(
        reader.preview(&id, None),
        Err(Error::PublicationPending(_))
    ));
    assert!(!reader.may_open(&id).unwrap());
    assert!(reader.path_to_open(&id).unwrap().is_none());
    reader.set_pin("2468", None).unwrap();
    assert!(matches!(
        reader.preview(&id, None),
        Err(Error::PublicationPending(_))
    ));
}

#[test]
fn cambio_de_contenido_no_se_publica_con_sensibilidad_anterior() {
    let mut f = Fixture::new("cambio", "Documento público sintético.");
    let library = f.library().to_path_buf();
    indexar::index_with(&mut f.writer, &[library], None, None).unwrap();
    std::fs::write(
        &f.file,
        format!("Ahora contiene una clave ficticia: {SECRET}."),
    )
    .unwrap();
    let id = f.stage_until_extracted();
    let reader = f.reader();
    assert!(reader.search(SECRET, 10).unwrap().hits.is_empty());
    assert!(reader.preview(&id, None).is_err());
    assert!(!reader.may_open(&id).unwrap());
    assert!(reader.path_to_open(&id).unwrap().is_none());
}

#[test]
fn fallo_preserva_la_version_anterior_y_reintenta_sin_perder_correccion() {
    let mut f = Fixture::new("fallo", "Texto anterior sintético recuperable.");
    let library = f.library().to_path_buf();
    indexar::index_with(&mut f.writer, std::slice::from_ref(&library), None, None).unwrap();
    let id = f
        .writer
        .find_subject_by_value(
            "path_observed_at",
            &f.file.to_string_lossy(),
            TimePoint::at(obrain_scan::now_ms()),
        )
        .unwrap()
        .unwrap();
    let brain = f.reader();
    let correction = brain
        .correct(&id, "sensitivity_level", Value::Text("sensitive".into()))
        .unwrap();
    drop(brain);

    std::fs::write(&f.file, format!("Versión nueva ficticia: {SECRET}.")).unwrap();
    assert_eq!(f.scan_only(), id);
    std::fs::remove_file(&f.file).unwrap();
    let files = load_files(&f.writer, TimePoint::at(obrain_scan::now_ms())).unwrap();
    let mut report = ExtractReport::default();
    f.writer
        .transaction(|s| extract_all_with(s, &files, obrain_scan::now_ms(), &mut report, None))
        .unwrap();
    assert_eq!(report.failed, 1);
    assert!(f.writer.content_pending(&id).unwrap());
    let previous = obrain_extract::index::document_text(f.writer.conn(), &id)
        .unwrap()
        .unwrap();
    assert!(previous.contains("anterior sintético"));
    let backup: i64 = f
        .writer
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM publication_backup_chunks WHERE entity=?1",
            [id.as_str()],
            |r| r.get(0),
        )
        .unwrap();
    assert!(backup > 0);
    let reader = f.reader();
    assert!(reader.search("anterior", 10).unwrap().hits.is_empty());
    assert!(reader.preview(&id, None).is_err());
    drop(reader);

    std::fs::write(&f.file, format!("Versión nueva ficticia: {SECRET}.")).unwrap();
    let reattempt = indexar::index_with(&mut f.writer, &[library], None, None).unwrap();
    assert!(reattempt.errors.is_empty(), "{:?}", reattempt.errors);
    assert!(!f.writer.content_pending(&id).unwrap());
    assert_eq!(
        f.writer
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM publication_backup_chunks WHERE entity=?1",
                [id.as_str()],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert!(
        f.writer
            .query(
                Some(&id),
                Some("sensitivity_level"),
                TimePoint::at(obrain_scan::now_ms())
            )
            .unwrap()
            .iter()
            .any(|a| a.id.as_str() == correction && a.method == Method::User)
    );
}

#[test]
fn fallo_de_clasificacion_conserva_cuarentena_y_admite_reintento() {
    let mut f = Fixture::new("clasificacion", &format!("Clave ficticia: {SECRET}."));
    let id = f.stage_until_extracted();
    f.writer
        .conn()
        .execute_batch(
            "CREATE TEMP TRIGGER abort_clasificacion
             BEFORE UPDATE OF sensitivity ON entities
             BEGIN SELECT RAISE(ABORT, 'fallo sintético de clasificación'); END",
        )
        .unwrap();
    let library = f.library().to_path_buf();
    assert!(
        indexar::index_with(&mut f.writer, std::slice::from_ref(&library), None, None).is_err()
    );
    assert!(f.writer.content_pending(&id).unwrap());
    let reader = f.reader();
    assert!(reader.search(SECRET, 10).unwrap().hits.is_empty());
    assert!(matches!(
        reader.preview(&id, None),
        Err(Error::PublicationPending(_))
    ));
    drop(reader);

    f.writer
        .conn()
        .execute_batch("DROP TRIGGER abort_clasificacion")
        .unwrap();
    let retry = indexar::index_with(&mut f.writer, &[library], None, None).unwrap();
    assert!(retry.errors.is_empty(), "{:?}", retry.errors);
    assert!(!f.writer.content_pending(&id).unwrap());
    assert_eq!(f.writer.sensitivity(&id).unwrap().as_str(), "vault");
    let reader = f.reader();
    reader.set_pin("2468", None).unwrap();
    reader.lock();
    let hits = reader.search(SECRET, 10).unwrap().hits;
    assert_eq!(hits.len(), 1);
    assert!(hits[0].snippet.is_empty());
}

#[test]
fn fallo_al_retractar_vencimiento_no_publica_el_contenido_nuevo() {
    let mut f = Fixture::new(
        "vencimiento-rollback",
        &format!("Clave ficticia: {SECRET}. VALIDEZ 15 11 2027."),
    );
    let library = f.library().to_path_buf();
    indexar::index_with(&mut f.writer, std::slice::from_ref(&library), None, None).unwrap();
    let id = f
        .writer
        .find_subject_by_value(
            "path_observed_at",
            &f.file.to_string_lossy(),
            TimePoint::at(obrain_scan::now_ms()),
        )
        .unwrap()
        .unwrap();
    let before = f
        .writer
        .resolve(&id, "expires_on", TimePoint::at(obrain_scan::now_ms()))
        .unwrap()
        .unwrap();
    assert_eq!(before.object.as_text(), Some("2027-11-15"));
    assert_eq!(f.writer.sensitivity(&id).unwrap().as_str(), "vault");

    std::fs::write(&f.file, "Documento público sin fecha.").unwrap();
    f.writer
        .conn()
        .execute_batch(
            "CREATE TEMP TRIGGER abort_expiry
             BEFORE UPDATE OF retracted_at ON assertions
             WHEN OLD.predicate='expires_on' AND OLD.source LIKE 'expiry%'
             BEGIN SELECT RAISE(ABORT, 'fallo sintético al retractar vencimiento'); END",
        )
        .unwrap();
    assert!(
        indexar::index_with(&mut f.writer, std::slice::from_ref(&library), None, None).is_err()
    );
    assert!(f.writer.content_pending(&id).unwrap());
    assert_eq!(f.writer.sensitivity(&id).unwrap().as_str(), "vault");
    let reader = f.reader();
    assert!(matches!(
        reader.preview(&id, None),
        Err(Error::PublicationPending(_))
    ));
    assert!(reader.expiring().unwrap().iter().all(|e| e.row.id != id.0));
    drop(reader);

    f.writer
        .conn()
        .execute_batch("DROP TRIGGER abort_expiry")
        .unwrap();
    let retry =
        indexar::index_with(&mut f.writer, std::slice::from_ref(&library), None, None).unwrap();
    assert_eq!(retry.pending, 0);
    assert!(!f.writer.content_pending(&id).unwrap());
    assert_eq!(f.writer.sensitivity(&id).unwrap().as_str(), "normal");
    assert!(
        f.writer
            .resolve(&id, "expires_on", TimePoint::at(obrain_scan::now_ms()))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        f.writer
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM assertions WHERE id=?1 AND retracted_at IS NOT NULL",
                [before.id.as_str()],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    let log = f.writer.log_len().unwrap();
    indexar::index_with(&mut f.writer, &[library], None, None).unwrap();
    assert_eq!(f.writer.log_len().unwrap(), log);
}

#[test]
fn lectura_concurrente_ve_texto_y_vencimiento_de_la_misma_version() {
    let mut f = Fixture::new(
        "vencimiento-concurrente",
        &format!("Clave ficticia: {SECRET}. VALIDEZ 15 11 2027."),
    );
    let library = f.library().to_path_buf();
    indexar::index_with(&mut f.writer, std::slice::from_ref(&library), None, None).unwrap();
    let id = f
        .writer
        .find_subject_by_value(
            "path_observed_at",
            &f.file.to_string_lossy(),
            TimePoint::at(obrain_scan::now_ms()),
        )
        .unwrap()
        .unwrap();
    let db = f.db.clone();
    let read_id = id.clone();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (resume_tx, resume_rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let store = Store::open(db).unwrap();
        store
            .read_snapshot(|s| -> obrain_core::Result<()> {
                assert!(!s.content_pending(&read_id)?);
                started_tx.send(()).unwrap();
                resume_rx.recv().unwrap();
                let at = TimePoint::at(obrain_scan::now_ms());
                assert_eq!(
                    s.resolve(&read_id, "expires_on", at)?
                        .unwrap()
                        .object
                        .as_text(),
                    Some("2027-11-15")
                );
                assert!(
                    obrain_extract::index::document_text(s.conn(), &read_id)?
                        .unwrap()
                        .contains(SECRET)
                );
                assert_eq!(s.sensitivity(&read_id)?.as_str(), "vault");
                Ok(())
            })
            .unwrap();
        let at = TimePoint::at(obrain_scan::now_ms());
        assert!(!store.content_pending(&read_id).unwrap());
        assert!(store.resolve(&read_id, "expires_on", at).unwrap().is_none());
        assert!(
            obrain_extract::index::document_text(store.conn(), &read_id)
                .unwrap()
                .unwrap()
                .contains("sin fecha")
        );
        assert_eq!(store.sensitivity(&read_id).unwrap().as_str(), "normal");
    });
    started_rx.recv().unwrap();
    std::fs::write(&f.file, "Documento público sin fecha.").unwrap();
    indexar::index_with(&mut f.writer, &[library], None, None).unwrap();
    resume_tx.send(()).unwrap();
    reader.join().unwrap();
}

#[test]
fn correccion_manual_de_fecha_sobrevive_al_cambio_de_contenido() {
    let mut f = Fixture::new(
        "vencimiento-corregido",
        &format!("Clave ficticia: {SECRET}. VALIDEZ 15 11 2027."),
    );
    let library = f.library().to_path_buf();
    indexar::index_with(&mut f.writer, std::slice::from_ref(&library), None, None).unwrap();
    let id = f
        .writer
        .find_subject_by_value(
            "path_observed_at",
            &f.file.to_string_lossy(),
            TimePoint::at(obrain_scan::now_ms()),
        )
        .unwrap()
        .unwrap();
    let reader = f.reader();
    let manual = reader.set_expiry(&id, Some("2025-12-01")).unwrap();
    drop(reader);
    std::fs::write(&f.file, "Documento público sin fecha.").unwrap();
    indexar::index_with(&mut f.writer, &[library], None, None).unwrap();
    let current = f
        .writer
        .query(
            Some(&id),
            Some("expires_on"),
            TimePoint::at(obrain_scan::now_ms()),
        )
        .unwrap();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].id.as_str(), manual);
    assert_eq!(current[0].method, Method::User);
    assert_eq!(current[0].object.as_text(), Some("2025-12-01"));
    let history: i64 = f
        .writer
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM assertions WHERE subject=?1 AND predicate='expires_on'",
            [id.as_str()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(history, 2);
}

#[test]
fn solo_el_documento_cambiado_entra_en_cuarentena() {
    let mut f = Fixture::new("aislado", "Primer documento sin secretos.");
    let library = f.library().to_path_buf();
    let other = f.library().join("otro.txt");
    std::fs::write(&other, "Segundo documento disponible.").unwrap();
    indexar::index_with(&mut f.writer, &[library], None, None).unwrap();
    std::fs::write(&f.file, format!("Contenido nuevo: {SECRET}.")).unwrap();
    f.stage_until_extracted();
    let reader = f.reader();
    assert!(reader.search(SECRET, 10).unwrap().hits.is_empty());
    assert_eq!(reader.search("Segundo", 10).unwrap().hits.len(), 1);
    assert_eq!(f.writer.content_pending_set().unwrap().len(), 1);
}

#[test]
fn apertura_externa_rechaza_bytes_modificados_antes_del_escaneo() {
    let mut f = Fixture::new("apertura", "Versión publicada sintética.");
    let library = f.library().to_path_buf();
    indexar::index_with(&mut f.writer, &[library], None, None).unwrap();
    let id = f
        .writer
        .find_subject_by_value(
            "path_observed_at",
            &f.file.to_string_lossy(),
            TimePoint::at(obrain_scan::now_ms()),
        )
        .unwrap()
        .unwrap();
    let reader = f.reader();
    assert!(reader.path_to_open(&id).unwrap().is_some());
    std::fs::write(&f.file, format!("Bytes nuevos: {SECRET}")).unwrap();
    assert!(reader.path_to_open(&id).unwrap().is_none());
}

#[test]
fn segunda_pasada_sin_cambios_no_reabre_cuarentena_ni_reescribe_log() {
    let mut f = Fixture::new("idempotente", "Documento sintético sin cambios.");
    let library = f.library().to_path_buf();
    indexar::index_with(&mut f.writer, std::slice::from_ref(&library), None, None).unwrap();
    let before = f.writer.log_len().unwrap();
    let report = indexar::index_with(&mut f.writer, &[library], None, None).unwrap();
    assert!(report.errors.is_empty());
    assert_eq!(report.pending, 0);
    assert_eq!(f.writer.log_len().unwrap(), before);
}

#[test]
fn renombrar_a_formato_no_extraible_cierra_la_version_textual_anterior() {
    let mut f = Fixture::new("extension", "Texto anterior ficticio.");
    let library = f.library().to_path_buf();
    indexar::index_with(&mut f.writer, std::slice::from_ref(&library), None, None).unwrap();
    let id = f
        .writer
        .find_subject_by_value(
            "path_observed_at",
            &f.file.to_string_lossy(),
            TimePoint::at(obrain_scan::now_ms()),
        )
        .unwrap()
        .unwrap();
    let renamed = library.join("notas.xyz");
    std::fs::rename(&f.file, &renamed).unwrap();
    f.file = renamed;
    let report = indexar::index_with(&mut f.writer, &[library], None, None).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(!f.writer.content_pending(&id).unwrap());
    assert!(f.reader().search("anterior", 10).unwrap().hits.is_empty());
    assert!(
        obrain_extract::index::document_text(f.writer.conn(), &id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn olvidar_un_pendiente_en_papelera_purga_tambien_el_respaldo() {
    let mut f = Fixture::new("olvido", "Texto publicado sintético.");
    let library = f.library().to_path_buf();
    indexar::index_with(&mut f.writer, std::slice::from_ref(&library), None, None).unwrap();
    let id = f
        .writer
        .find_subject_by_value(
            "path_observed_at",
            &f.file.to_string_lossy(),
            TimePoint::at(obrain_scan::now_ms()),
        )
        .unwrap()
        .unwrap();
    std::fs::write(&f.file, "Texto cambiado sintético.").unwrap();
    assert_eq!(f.stage_until_extracted(), id);
    assert!(
        f.writer
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM publication_backup_chunks WHERE entity=?1",
                [id.as_str()],
                |r| r.get::<_, i64>(0)
            )
            .unwrap()
            > 0
    );
    std::fs::remove_file(&f.file).unwrap();
    indexar::index_with(&mut f.writer, &[library], None, None).unwrap();
    let mut reader = f.reader();
    assert_eq!(reader.forget(Some(&id)).unwrap(), 1);
    assert!(!reader.store().content_pending(&id).unwrap());
    assert_eq!(
        reader
            .store()
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM publication_backup_chunks WHERE entity=?1",
                [id.as_str()],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn miniatura_espera_a_la_puerta_y_verifica_los_bytes_publicados() {
    let mut f = Fixture::new("miniatura", "Documento auxiliar sintético.");
    let image = f.library().join("captura.png");
    image::RgbImage::from_pixel(4, 4, image::Rgb([10, 20, 30]))
        .save(&image)
        .unwrap();
    let scanner = Scanner::new();
    let (mut candidates, mut scan) = scanner.scan(f.library());
    scanner.hash_all(&mut candidates, &mut scan);
    assert!(scan.errors.is_empty());
    let mut ingested = IngestReport::default();
    f.writer
        .transaction(|s| ingest(s, &candidates, obrain_scan::now_ms(), &mut ingested))
        .unwrap();
    let id = f
        .writer
        .find_subject_by_value(
            "path_observed_at",
            &image.to_string_lossy(),
            TimePoint::at(obrain_scan::now_ms()),
        )
        .unwrap()
        .unwrap();
    let reader = f.reader();
    assert!(reader.preview(&id, None).is_err());
    assert!(reader.path_to_open(&id).unwrap().is_none());
    drop(reader);

    let library = f.library().to_path_buf();
    indexar::index_with(&mut f.writer, &[library], None, None).unwrap();
    let reader = f.reader();
    assert_eq!(reader.preview(&id, None).unwrap().kind, "image");
    image::RgbImage::from_pixel(4, 4, image::Rgb([50, 60, 70]))
        .save(&image)
        .unwrap();
    assert!(reader.preview(&id, None).unwrap().image.is_none());
    assert!(reader.path_to_open(&id).unwrap().is_none());
}

#[test]
fn migracion_de_copia_sintetica_conserva_log_y_texto() {
    let mut f = Fixture::new("migracion", "Documento sintético migrable.");
    let library = f.library().to_path_buf();
    indexar::index_with(&mut f.writer, &[library], None, None).unwrap();
    let id = f
        .writer
        .find_subject_by_value(
            "path_observed_at",
            &f.file.to_string_lossy(),
            TimePoint::at(obrain_scan::now_ms()),
        )
        .unwrap()
        .unwrap();
    let correction = f
        .reader()
        .correct(&id, "sensitivity_level", Value::Text("sensitive".into()))
        .unwrap();
    let log_before = f.writer.log_len().unwrap();
    let text_before = obrain_extract::index::document_text(f.writer.conn(), &id).unwrap();
    f.writer
        .conn()
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    let copy = f.root.join("legacy-copy.db");
    std::fs::copy(&f.db, &copy).unwrap();
    let source_bytes = std::fs::read(&f.db).unwrap();
    {
        let legacy = Store::open(&copy).unwrap();
        legacy.conn().execute_batch(
            "DROP TABLE content_publication; DROP TABLE publication_backup_chunks; PRAGMA user_version=1"
        ).unwrap();
    }
    let migrated = Store::open(&copy).unwrap();
    assert!(migrated.content_pending(&id).unwrap());
    assert_eq!(migrated.log_len().unwrap(), log_before);
    assert_eq!(
        obrain_extract::index::document_text(migrated.conn(), &id).unwrap(),
        text_before
    );
    assert_eq!(
        migrated
            .conn()
            .query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    drop(migrated);
    // Reabrir la copia es idempotente; la fuente sintética sigue intacta.
    let reopened = Store::open(&copy).unwrap();
    assert!(reopened.content_pending(&id).unwrap());
    // Simula un corte tras crear las tablas, antes de completar el backfill.
    reopened
        .conn()
        .execute(
            "DELETE FROM content_publication WHERE entity=?1",
            [id.as_str()],
        )
        .unwrap();
    drop(reopened);
    let repaired = Store::open(&copy).unwrap();
    assert!(repaired.content_pending(&id).unwrap());
    drop(repaired);
    let reader = Brain::open(&copy).unwrap();
    assert!(
        reader
            .preview(&id, None)
            .unwrap()
            .text
            .unwrap()
            .contains("migrable")
    );
    assert_eq!(
        reader.store().sensitivity(&id).unwrap().as_str(),
        "sensitive"
    );
    assert!(
        reader
            .store()
            .query(
                Some(&id),
                Some("sensitivity_level"),
                TimePoint::at(obrain_scan::now_ms())
            )
            .unwrap()
            .iter()
            .any(|a| a.id.as_str() == correction && a.method == Method::User)
    );
    assert!(!reader.store().content_pending(&id).unwrap());
    assert_eq!(std::fs::read(&f.db).unwrap(), source_bytes);
}

#[test]
fn lectura_concurrente_mantiene_una_instantanea_coherente() {
    let mut f = Fixture::new("concurrente", "Versión pública sintética.");
    let library = f.library().to_path_buf();
    indexar::index_with(&mut f.writer, &[library], None, None).unwrap();
    let id = f
        .writer
        .find_subject_by_value(
            "path_observed_at",
            &f.file.to_string_lossy(),
            TimePoint::at(obrain_scan::now_ms()),
        )
        .unwrap()
        .unwrap();
    let db = f.db.clone();
    let read_id = id.clone();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (resume_tx, resume_rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let store = Store::open(db).unwrap();
        store
            .read_snapshot(|s| -> obrain_core::Result<()> {
                assert!(!s.content_pending(&read_id)?);
                started_tx.send(()).unwrap();
                resume_rx.recv().unwrap();
                assert!(!s.content_pending(&read_id)?);
                assert!(
                    obrain_extract::index::document_text(s.conn(), &read_id)?
                        .unwrap()
                        .contains("pública")
                );
                Ok(())
            })
            .unwrap();
        assert!(store.content_pending(&read_id).unwrap());
    });
    started_rx.recv().unwrap();
    f.writer
        .transaction(|s| s.prepare_content(&id, Some("nuevo-hash-sintético")))
        .unwrap();
    resume_tx.send(()).unwrap();
    reader.join().unwrap();
}
