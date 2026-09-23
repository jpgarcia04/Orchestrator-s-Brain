//! `multilingual-e5-small` sobre ONNX Runtime. La única implementación de
//! [`Embedder`] que hay hoy, y deliberadamente la única cosa de este crate que
//! sabe que existe un modelo.
//!
//! ## Por qué este modelo
//!
//! El corpus real de esta máquina es **castellano, inglés y código fuente
//! mezclados** —`REVISION y PROPUESTA de Mejora al Almacén`, `bindings_main.js`
//! y `PROJECT-MASTER.md` en el mismo índice—, así que un modelo monolingüe
//! inglés queda descartado antes de medir nada. El razonamiento completo, con
//! las alternativas descartadas, está en `docs/adr/0007`.
//!
//! ## La trampa de los prefijos
//!
//! E5 se entrenó con `query: ` y `passage: ` delante, y **no son
//! intercambiables**. Sin ellos el modelo sigue produciendo vectores de aspecto
//! razonable y la calidad baja sin que nada falle. Por eso el prefijo no es un
//! detalle de esta implementación sino [`Role`], que viaja en el contrato.

use crate::embedder::{Embedder, ModelId, Role, normalize};
use crate::{Error, Result};
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;
use tokenizers::Tokenizer;

/// `version` es de la **integración**, no del repositorio de HuggingFace.
/// Cambiar los prefijos, el pooling o [`MAX_TOKENS`] cambia los vectores y
/// obliga a subirla: eso es lo que I7 compra.
pub const MODEL: ModelId = ModelId {
    id: "multilingual-e5-small",
    version: 1,
    dim: 384,
};

/// El modelo tiene 512 posiciones. Un fragmento de `paragraph-pack@1` son
/// 1.200 bytes, que en castellano rondan los 300 tokens: cabe con holgura y el
/// truncado casi nunca se activa. El día que el chunker cambie, este número y
/// el suyo tienen que mirarse juntos.
pub const MAX_TOKENS: usize = 512;

/// Los tres archivos que hacen falta. No hay un cuarto escondido: si estos
/// están, el pipeline funciona sin red para siempre.
pub const FILES: &[&str] = &["model.onnx", "tokenizer.json", "config.json"];

pub struct E5 {
    session: std::cell::RefCell<Session>,
    tokenizer: Tokenizer,
}

impl E5 {
    /// Carga el modelo de un directorio local. **No toca la red.**
    ///
    /// Que esta función no pueda descargar nada es la mitad de la promesa de
    /// F1: la otra mitad es que exista un paso explícito que sí lo haga
    /// ([`crate::download`]), y que sea el usuario quien lo ejecute.
    pub fn load(dir: &Path) -> Result<Self> {
        for f in FILES {
            let p = dir.join(f);
            if !p.exists() {
                return Err(Error::ModelMissing(p.display().to_string()));
            }
        }
        let tokenizer = Tokenizer::from_file(dir.join("tokenizer.json"))
            .map_err(|e| Error::Model(format!("tokenizer: {e}")))?;
        // `recover()` y no `?`: la optimización del grafo no existe en las
        // compilaciones mínimas de ONNX Runtime, y quedarse sin ella es más
        // lento, no incorrecto. Es el idioma que documenta la propia librería.
        let session = Session::builder()?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
            .unwrap_or_else(|e| e.recover())
            .commit_from_file(dir.join("model.onnx"))?;
        Ok(E5 {
            session: std::cell::RefCell::new(session),
            tokenizer,
        })
    }

    /// El texto tal y como lo ve el modelo. Público porque es lo que hay que
    /// poder probar: el prefijo es la diferencia entre una búsqueda semántica
    /// buena y una mediocre, y no se ve en ningún contador.
    pub fn prepare(text: &str, role: Role) -> String {
        match role {
            Role::Query => format!("query: {text}"),
            Role::Passage => format!("passage: {text}"),
        }
    }
}

impl Embedder for E5 {
    fn model(&self) -> ModelId {
        MODEL
    }

    fn embed(&self, texts: &[&str], role: Role) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let prepared: Vec<String> = texts.iter().map(|t| E5::prepare(t, role)).collect();
        let encodings = self
            .tokenizer
            .encode_batch(prepared, true)
            .map_err(|e| Error::Model(format!("tokenizar: {e}")))?;

        // Relleno hasta el más largo del lote, no hasta 512: un lote de
        // fragmentos cortos no debe pagar el coste del caso peor.
        let seq = encodings
            .iter()
            .map(|e| e.get_ids().len().min(MAX_TOKENS))
            .max()
            .unwrap_or(1)
            .max(1);
        let batch = encodings.len();

        let mut ids = vec![0i64; batch * seq];
        let mut mask = vec![0i64; batch * seq];
        for (b, enc) in encodings.iter().enumerate() {
            let n = enc.get_ids().len().min(seq);
            for t in 0..n {
                ids[b * seq + t] = enc.get_ids()[t] as i64;
                mask[b * seq + t] = enc.get_attention_mask()[t] as i64;
            }
        }
        // El grafo declara `token_type_ids` aunque este modelo sea de una sola
        // secuencia. Se pasa a cero en vez de omitirlo: ONNX Runtime exige
        // todas las entradas declaradas.
        let types = vec![0i64; batch * seq];

        let shape = vec![batch as i64, seq as i64];
        let mut session = self.session.borrow_mut();
        let outputs = session.run(ort::inputs![
            "input_ids"      => Tensor::from_array((shape.clone(), ids))?,
            "attention_mask" => Tensor::from_array((shape.clone(), mask.clone()))?,
            "token_type_ids" => Tensor::from_array((shape, types))?,
        ])?;

        let (out_shape, hidden) = outputs["last_hidden_state"].try_extract_tensor::<f32>()?;
        let dim = *out_shape
            .last()
            .ok_or_else(|| Error::Model("salida sin forma".into()))? as usize;
        if dim != MODEL.dim {
            return Err(Error::Model(format!(
                "el modelo devuelve {dim} dimensiones, no {}",
                MODEL.dim
            )));
        }

        // ── mean pooling con la máscara ──
        // Promediar incluyendo el relleno mete el vector del token `<pad>` en
        // la media, y el resultado depende de cuántos vecinos había en el lote
        // en vez de sólo del texto. Un fallo así no se ve: los vectores salen
        // y el mismo fragmento embebido en dos lotes distintos da dos vectores
        // distintos. Ver el test `el_lote_no_cambia_el_vector`.
        let mut out = Vec::with_capacity(batch);
        for b in 0..batch {
            let mut v = vec![0f32; dim];
            let mut n = 0f32;
            for t in 0..seq {
                if mask[b * seq + t] == 0 {
                    continue;
                }
                let base = (b * seq + t) * dim;
                for (d, x) in v.iter_mut().enumerate() {
                    *x += hidden[base + d];
                }
                n += 1.0;
            }
            if n > 0.0 {
                for x in v.iter_mut() {
                    *x /= n;
                }
            }
            normalize(&mut v);
            out.push(v);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La trampa del modelo, convertida en test. Si alguien "simplifica" los
    /// prefijos, esto se pone rojo antes de que la calidad baje en silencio.
    #[test]
    fn la_consulta_y_el_documento_no_se_preparan_igual() {
        let t = "certificado de ingresos";
        assert_eq!(
            E5::prepare(t, Role::Query),
            "query: certificado de ingresos"
        );
        assert_eq!(
            E5::prepare(t, Role::Passage),
            "passage: certificado de ingresos"
        );
        assert_ne!(E5::prepare(t, Role::Query), E5::prepare(t, Role::Passage));
    }
}
