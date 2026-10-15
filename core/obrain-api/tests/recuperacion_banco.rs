//! Banco de calidad: mide recuperación y suficiencia de citas por separado.
//! E5 sólo se ejecuta explícitamente y nunca se descarga desde una prueba.
use obrain_api::{Brain, indexar, load_model};
use obrain_core::{EntityId, Store};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Deserialize)]
struct Expected {
    document: String,
    answer: String,
}

#[derive(Deserialize)]
struct Question {
    id: String,
    kind: String,
    query: String,
    expected: Vec<Expected>,
    lexical_required: bool,
}

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn copy_fixture(source: &Path, dest: &Path) {
    std::fs::create_dir_all(dest).unwrap();
    for item in std::fs::read_dir(source).unwrap() {
        let item = item.unwrap();
        let path = item.path();
        if path.file_name().is_some_and(|s| s == "README.md") {
            continue;
        }
        if path.is_dir() {
            copy_fixture(&path, &dest.join(item.file_name()));
        } else {
            std::fs::copy(&path, dest.join(item.file_name())).unwrap();
        }
    }
}

#[derive(Serialize)]
struct Candidate {
    document: String,
    passages: Vec<String>,
    ranks: Vec<(String, usize)>,
}

#[derive(Serialize)]
struct Measurement {
    id: String,
    kind: String,
    query: String,
    expected_documents: Vec<String>,
    expected_answers: Vec<String>,
    top3: Vec<String>,
    evidence_top3: Vec<Candidate>,
    first_relevant_rank: Option<usize>,
    supporting_passage_top3: bool,
    candidates_without_answer: bool,
    elapsed_ms: f64,
}

fn evaluate(model: Option<&Path>) {
    let questions: Vec<Question> = serde_json::from_str(include_str!(
        "../../../tests/fixtures/recuperacion/preguntas.json"
    ))
    .unwrap();
    let fixture = Fixture(std::env::temp_dir().join(format!("obrain-banco-{}", EntityId::new())));
    copy_fixture(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/producto-windows"),
        &fixture.0,
    );
    let mut store = Store::open_in_memory().unwrap();
    let embedder = model.map(|dir| {
        load_model(Some(dir))
            .expect("no se pudo cargar E5")
            .expect("faltan archivos locales de E5; no se permite fallback léxico")
    });
    // Comprueba que las respuestas de referencia existen realmente en el
    // corpus indexado; un error del banco no debe parecer un fallo del buscador.
    let started = Instant::now();
    let indexing = indexar::index_with(
        &mut store,
        std::slice::from_ref(&fixture.0),
        embedder.as_deref().map(|e| e as &dyn obrain_api::Embedder),
        None,
    )
    .unwrap();
    assert!(
        indexing.errors.is_empty() && indexing.deferred.is_empty(),
        "{indexing:?}"
    );
    let indexing_ms = started.elapsed().as_secs_f64() * 1000.0;
    let brain = match embedder {
        Some(e) => Brain::with_embedder(store, e).unwrap(),
        None => Brain::try_from_store(store).unwrap(),
    };
    let vectors: usize = if model.is_some() {
        brain
            .store()
            .conn()
            .query_row("SELECT COUNT(*) FROM vectors", [], |r| r.get(0))
            .unwrap()
    } else {
        0
    };
    assert!(
        model.is_none() || vectors > 0,
        "E5 cargado pero sin vectores indexados"
    );
    let mut observed_knn = false;
    for q in &questions {
        for gold in &q.expected {
            let mut paths = Vec::new();
            fn find(root: &Path, name: &str, out: &mut Vec<PathBuf>) {
                for entry in std::fs::read_dir(root).unwrap() {
                    let path = entry.unwrap().path();
                    if path.is_dir() {
                        find(&path, name, out);
                    } else if path.file_name().is_some_and(|n| n == name) {
                        out.push(path);
                    }
                }
            }
            find(&fixture.0, &gold.document, &mut paths);
            assert_eq!(
                paths.len(),
                1,
                "referencia ambigua o ausente: {}",
                gold.document
            );
            let id = brain
                .document_at(&paths[0].to_string_lossy())
                .unwrap()
                .unwrap();
            let text = obrain_extract::index::document_text(brain.store().conn(), &id)
                .unwrap()
                .unwrap();
            assert!(
                text.contains(&gold.answer),
                "respuesta ausente del corpus: {}",
                q.id
            );
        }
    }
    let mut rows = Vec::new();
    let mut failures = Vec::new();
    for q in questions {
        let started = Instant::now();
        let answer = brain.search(&q.query, 10).unwrap();
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(
            answer.semantic,
            model.is_some(),
            "{}: rama incorrecta",
            q.id
        );
        if model.is_some() {
            assert!(answer.branches.contains(&"knn"));
        }
        observed_knn |= answer
            .hits
            .iter()
            .any(|h| h.ranks.iter().any(|(branch, _)| branch == "knn"));
        let rank = answer
            .hits
            .iter()
            .position(|h| q.expected.iter().any(|e| e.document == h.label))
            .map(|n| n + 1);
        let supporting = answer.hits.iter().take(3).any(|h| {
            q.expected.iter().any(|e| {
                e.document == h.label && h.passages.iter().any(|p| p.text.contains(&e.answer))
            })
        });
        // Toda cita debe ser literal, tener offsets UTF-8 válidos y abrirse en
        // esa misma versión. Que mencione una palabra no basta para darla por respuesta.
        for h in &answer.hits {
            let id = EntityId(h.entity.clone());
            let text = obrain_extract::index::document_text(brain.store().conn(), &id)
                .unwrap()
                .unwrap();
            for p in &h.passages {
                assert_eq!(
                    text.get(p.span[0]..p.span[1]),
                    Some(p.text.as_str()),
                    "{}",
                    q.id
                );
                let preview = brain
                    .preview_at_version(
                        &id,
                        None,
                        Some((p.span[0], p.span[1])),
                        Some(&p.published_hash),
                        Some(&p.text_hash),
                    )
                    .unwrap();
                let (start, end) = preview.highlight.expect("sin ubicación de cita");
                assert_eq!(
                    preview.text.unwrap().get(start..end),
                    Some(p.text.as_str()),
                    "{}",
                    q.id
                );
            }
        }
        // Umbral sólo para casos léxicos ya cubiertos: las paráfrasis y la
        // comparación E5 se miden sin convertir expectativas en resultados.
        if model.is_none() && q.lexical_required && (!rank.is_some_and(|n| n <= 3) || !supporting) {
            failures.push(q.id.clone());
        }
        rows.push(Measurement {
            id: q.id,
            kind: q.kind,
            query: q.query,
            expected_documents: q.expected.iter().map(|e| e.document.clone()).collect(),
            expected_answers: q.expected.iter().map(|e| e.answer.clone()).collect(),
            top3: answer
                .hits
                .iter()
                .take(3)
                .map(|h| h.label.clone())
                .collect(),
            evidence_top3: answer
                .hits
                .iter()
                .take(3)
                .map(|h| Candidate {
                    document: h.label.clone(),
                    passages: h.passages.iter().map(|p| p.text.clone()).collect(),
                    ranks: h.ranks.clone(),
                })
                .collect(),
            first_relevant_rank: rank,
            supporting_passage_top3: supporting,
            candidates_without_answer: q.expected.is_empty() && !answer.hits.is_empty(),
            elapsed_ms,
        });
    }
    // El banco también se consulta bloqueado: ningún resultado protegido puede
    // filtrar el contenido que acabamos de recibir sin PIN.
    brain.set_pin("482916", None).unwrap();
    brain.lock();
    for row in &rows {
        for h in brain.search(&row.query, 10).unwrap().hits {
            if h.sensitivity != "normal" {
                assert!(
                    h.snippet.is_empty() && h.passages.is_empty(),
                    "{}: PIN",
                    row.id
                );
            }
        }
    }
    assert!(
        model.is_none() || observed_knn,
        "la evaluación no recuperó ningún resultado kNN"
    );
    let answerable = rows
        .iter()
        .filter(|r| !r.expected_documents.is_empty())
        .count();
    let report = serde_json::json!({
        "schema": 1,
        "mode": if model.is_some() { "hybrid-e5" } else { "lexical" },
        "model": model.map(|_| "multilingual-e5-small@integration-1"),
        "indexing_ms": indexing_ms,
        "vectors": vectors,
        "questions": rows.len(),
        "answerable": answerable,
        "document_top1": rows.iter().filter(|r| r.first_relevant_rank == Some(1)).count(),
        "document_top3": rows.iter().filter(|r| r.first_relevant_rank.is_some_and(|n| n <= 3)).count(),
        "supporting_passage_top3": rows.iter().filter(|r| r.supporting_passage_top3).count(),
        "unanswerable": rows.iter().filter(|r| r.expected_documents.is_empty()).count(),
        "unanswerable_with_candidates": rows.iter().filter(|r| r.candidates_without_answer).count(),
        "lexical_required_failures": failures,
        "results": rows,
    });
    let json = serde_json::to_string_pretty(&report).unwrap();
    if let Some(dest) = std::env::var_os("OBRAIN_EVAL_REPORT") {
        std::fs::write(dest, &json).unwrap();
    }
    println!("{json}");
    assert!(failures.is_empty(), "regresiones de calidad: {failures:?}");
}

#[test]
fn evaluar_recuperacion_lexica() {
    evaluate(None);
}

#[test]
#[ignore = "requiere OBRAIN_EVAL_MODEL con E5 real descargado explícitamente"]
fn evaluar_hibrida_e5() {
    let dir = std::env::var_os("OBRAIN_EVAL_MODEL").expect("falta OBRAIN_EVAL_MODEL");
    evaluate(Some(Path::new(&dir)));
}
