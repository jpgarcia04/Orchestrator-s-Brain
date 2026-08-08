//! El contrato `Embedder` de `PROJECT-MASTER.md` §5.1: `text[] → vector[]`.
//!
//! Lo que hace que esto sea una capacidad y no un envoltorio de una API
//! concreta es lo que **no** aparece en la firma: ni el nombre del proveedor,
//! ni el formato del modelo, ni la ruta de unos pesos. Un llamante pide
//! vectores para unos textos y dice para qué son.
//!
//! ## Por qué `Role` está en el contrato y no en la implementación
//!
//! `multilingual-e5-small` exige prefijar la entrada con `query: ` o
//! `passage: `, y los dos son distintos. Es la clase de detalle que degrada la
//! calidad sin romper nada: los vectores salen, la búsqueda devuelve
//! resultados, y son peores sin que ningún test lo note.
//!
//! Se puede resolver de dos maneras. La cómoda es documentarlo y confiar en
//! que quien llame se acuerde. La otra es meter el rol en la firma, que es lo
//! que se hace aquí: no existe un `embed(texts)` a secas al que olvidársele el
//! prefijo. Un modelo que no los necesite ignora el `Role` y no pierde nada;
//! uno que sí, no puede recibirlo mal.

use crate::Result;

/// Para qué es este vector. La asimetría es real en los modelos de
/// recuperación modernos: la consulta y el documento se embeben distinto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Lo que escribe una persona en la barra.
    Query,
    /// Un fragmento del corpus que se indexa.
    Passage,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Query => "query",
            Role::Passage => "passage",
        }
    }
}

/// La identidad de un modelo, tal y como la exige el invariante I7.
///
/// Va **dentro de cada vector**, no en una tabla de configuración. Es la
/// diferencia entre poder cambiar de modelo dentro de dos años con un
/// `rebuild` en paralelo y tener que tirar el índice entero sin saber qué
/// había dentro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelId {
    pub id: &'static str,
    /// Versión de **esta integración**, no del repositorio de origen. Sube si
    /// cambia cualquier cosa que altere los vectores: los pesos, los prefijos,
    /// el pooling, la longitud máxima.
    pub version: i64,
    pub dim: usize,
}

impl ModelId {
    /// Etiqueta para el log y para los informes.
    pub fn label(&self) -> String {
        format!("{}@{}", self.id, self.version)
    }
}

/// La capacidad. `PROJECT-MASTER.md` §5.1: `text[] → vector[]`, local viable.
pub trait Embedder {
    fn model(&self) -> ModelId;

    /// Vectores **normalizados a norma 1**, en el mismo orden que la entrada.
    ///
    /// La normalización es parte del contrato y no una decisión de la
    /// implementación: con vectores unitarios el coseno *es* el producto
    /// escalar, y el índice puede buscar sin volver a dividir por normas que
    /// ya conocía. Un implementador que devuelva vectores sin normalizar
    /// rompe el kNN de forma silenciosa.
    fn embed(&self, texts: &[&str], role: Role) -> Result<Vec<Vec<f32>>>;

    /// Cuántos textos conviene pasar de una vez.
    fn batch_size(&self) -> usize {
        16
    }
}

/// Coseno de dos vectores **ya normalizados**: el producto escalar.
///
/// No comprueba las normas a propósito. Es la operación del bucle interior del
/// kNN y se ejecuta una vez por vector del índice; el sitio de comprobar que
/// un vector es unitario es cuando se escribe, no cuando se lee un millón de
/// veces. Ver [`crate::index::insert`].
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Normaliza en el sitio. Un vector de norma cero se deja como está: dividir
/// por cero daría `NaN`, y un `NaN` en el índice envenena todas las
/// comparaciones en vez de estropear una.
pub fn normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > f32::EPSILON {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizar_deja_norma_uno() {
        let mut v = vec![3.0, 4.0];
        normalize(&mut v);
        assert!((v.iter().map(|x| x * x).sum::<f32>() - 1.0).abs() < 1e-6);
        assert!((cosine(&v, &v) - 1.0).abs() < 1e-6);
    }

    /// Un vector nulo no puede envenenar el índice con `NaN`.
    #[test]
    fn un_vector_nulo_no_produce_nan() {
        let mut v = vec![0.0, 0.0, 0.0];
        normalize(&mut v);
        assert!(v.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn el_coseno_de_ortogonales_es_cero() {
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
        assert!((cosine(&[1.0, 0.0], &[-1.0, 0.0]) + 1.0).abs() < 1e-6);
    }
}
