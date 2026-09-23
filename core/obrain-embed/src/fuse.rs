//! Reciprocal Rank Fusion. `PROJECT-MASTER.md` §6, paso 3.
//!
//! ```text
//! score(d) = Σ  1 / (k + rank_i(d))        k = 60
//!            i
//! ```
//!
//! ## Por qué RRF y no una media ponderada
//!
//! BM25 devuelve números negativos sin cota —en este corpus, entre −0,5 y
//! −13— y el coseno devuelve valores en [−1, 1] que además se apelotonan
//! arriba: en un espacio de embeddings los vecinos plausibles están casi todos
//! entre 0,80 y 0,90. **No son comparables ni normalizándolos**, porque la
//! normalización sólo traslada el problema a decidir contra qué se normaliza:
//! ¿el máximo de esta consulta? Entonces la puntuación de un documento depende
//! de sus rivales. ¿Un máximo global? Entonces depende del corpus.
//!
//! RRF tira los valores y se queda sólo con el **orden**, que es lo único que
//! las dos ramas producen de forma comparable. Y no tiene pesos que ajustar,
//! que en un corpus de 800 archivos es una virtud y no una limitación: unos
//! pesos afinados a mano aquí estarían sobreajustados a este disco.
//!
//! ## Por qué `k = 60`
//!
//! Es el valor del artículo original de Cormack, Clarke y Buettcher (2009) y
//! el que usan por defecto Elasticsearch y OpenSearch. Amortigua las primeras
//! posiciones: con k=60 la diferencia entre el puesto 1 y el 2 es del 1,6 %,
//! no del 50 %. Eso es lo que se busca — que una rama muy segura de sí misma
//! no aplaste a la otra por un solo puesto.

use std::collections::HashMap;
use std::hash::Hash;

/// La constante de §6. Se deja pública porque un informe que dice «fusión RRF»
/// sin decir con qué `k` no ha dicho nada.
pub const K: f64 = 60.0;

/// Una rama de recuperación: su nombre y sus resultados **ya ordenados**.
pub struct Ranking<'a, T> {
    pub name: &'a str,
    pub items: Vec<T>,
}

/// Qué aportó cada rama a un resultado fusionado. Es la procedencia de I9
/// aplicada a la búsqueda: «esto sale el tercero porque BM25 lo puso primero
/// y el vectorial decimoquinto» es una respuesta; «0,032» no lo es.
#[derive(Debug, Clone)]
pub struct Fused<T> {
    pub item: T,
    pub score: f64,
    /// `(rama, puesto)`, empezando en 1. Sólo las ramas que lo devolvieron.
    pub ranks: Vec<(String, usize)>,
}

/// Fusiona. El orden de las ramas no influye en el resultado.
pub fn rrf<T: Clone + Eq + Hash>(rankings: &[Ranking<'_, T>], limit: usize) -> Vec<Fused<T>> {
    let mut scores: HashMap<T, (f64, Vec<(String, usize)>)> = HashMap::new();
    for r in rankings {
        for (i, item) in r.items.iter().enumerate() {
            let rank = i + 1;
            let e = scores.entry(item.clone()).or_insert((0.0, Vec::new()));
            e.0 += 1.0 / (K + rank as f64);
            e.1.push((r.name.to_string(), rank));
        }
    }
    let mut out: Vec<Fused<T>> = scores
        .into_iter()
        .map(|(item, (score, mut ranks))| {
            ranks.sort_by_key(|(n, _)| n.clone());
            Fused { item, score, ranks }
        })
        .collect();
    // Desempate por el mejor puesto conseguido en cualquier rama, y luego por
    // el número de ramas. Sin él, dos empates devuelven un orden que depende
    // del hash y la salida deja de ser reproducible entre ejecuciones.
    out.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| {
                let ma = a.ranks.iter().map(|(_, r)| *r).min().unwrap_or(usize::MAX);
                let mb = b.ranks.iter().map(|(_, r)| *r).min().unwrap_or(usize::MAX);
                ma.cmp(&mb)
            })
            .then_with(|| b.ranks.len().cmp(&a.ranks.len()))
    });
    out.truncate(limit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r<'a>(name: &'a str, items: &[&str]) -> Ranking<'a, String> {
        Ranking {
            name,
            items: items.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Lo que RRF compra: un documento que las dos ramas ponen segundo gana a
    /// uno que una sola pone primero. Es el comportamiento que hace que la
    /// fusión valga la pena, y el que hay que enseñar en el informe.
    #[test]
    fn el_acuerdo_de_dos_ramas_gana_a_la_certeza_de_una() {
        let f = rrf(
            &[
                r("bm25", &["solo-lexico", "acordado"]),
                r("knn", &["solo-vectorial", "acordado"]),
            ],
            10,
        );
        assert_eq!(f[0].item, "acordado");
        assert_eq!(f[0].ranks.len(), 2);
        // 2/(60+2) = 0.03226 frente a 1/(60+1) = 0.01639
        assert!(f[0].score > f[1].score);
    }

    /// Un documento que sólo encuentra la rama vectorial sigue apareciendo.
    /// Es el criterio de "hecho" de M4: encontrar por significado algo que
    /// BM25 no encuentra. Si la fusión lo enterrara, el hito no serviría.
    #[test]
    fn lo_que_solo_ve_una_rama_no_desaparece() {
        let f = rrf(&[r("bm25", &["a", "b"]), r("knn", &["c"])], 10);
        assert!(f.iter().any(|x| x.item == "c"));
    }

    #[test]
    fn el_orden_de_las_ramas_no_importa() {
        let a = rrf(&[r("bm25", &["x", "y"]), r("knn", &["y", "z"])], 10);
        let b = rrf(&[r("knn", &["y", "z"]), r("bm25", &["x", "y"])], 10);
        let n = |v: &[Fused<String>]| v.iter().map(|f| f.item.clone()).collect::<Vec<_>>();
        assert_eq!(n(&a), n(&b));
    }

    /// Reproducibilidad: sin el desempate explícito el orden dependería del
    /// hash y dos ejecuciones darían listas distintas.
    #[test]
    fn los_empates_se_rompen_siempre_igual() {
        let una = rrf(&[r("bm25", &["p", "q"])], 10);
        for _ in 0..20 {
            let otra = rrf(&[r("bm25", &["p", "q"])], 10);
            assert_eq!(una[0].item, otra[0].item);
            assert_eq!(una[1].item, otra[1].item);
        }
    }

    #[test]
    fn una_rama_vacia_no_cambia_nada() {
        let f = rrf(&[r("bm25", &["a"]), r("knn", &[])], 10);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].item, "a");
    }
}
