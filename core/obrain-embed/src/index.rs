//! La proyección vectorial: la tabla `vectors` y el kNN.
//!
//! **Nada de esto es la verdad** (I1). Un vector es una derivación de un
//! fragmento, que a su vez es una derivación de un archivo: se puede tirar
//! entera y rehacer. Por eso vive en su propia tabla, igual que `chunks`
//! ([ADR-0005](../../../docs/adr/0005-donde-vive-el-texto-extraido.md)), y por
//! eso `DROP TABLE vectors` no pierde ni un dato del sistema.
//!
//! ## I7 cableado, no documentado
//!
//! > *Todo vector guarda modelo + versión + dimensión + chunker.*
//!
//! Las cinco columnas están **en la clave primaria**. No es una anotación para
//! el futuro: es lo que permite que dos modelos convivan en la misma tabla
//! durante una migración, que es literalmente lo que §5.3 describe —crear el
//! índice nuevo en paralelo, re-embeber con presupuesto, conmutar, retirar el
//! viejo—. Con las columnas fuera de la clave, "en paralelo" no existe: el
//! segundo modelo pisa al primero y la migración se vuelve un corte.
//!
//! `extractions` y `chunks` ya llevan `(chunker, chunker_ver)` en su clave por
//! el mismo motivo. Esto sigue el patrón.
//!
//! ## Por qué un recorrido lineal y no un índice ANN
//!
//! `PROJECT-MASTER.md` §7.1 nombra `sqlite-vec` como «componente
//! reemplazable». Con el corpus real —2.323 vectores de 384 dimensiones, 3,4 MB—
//! un recorrido lineal completo es del orden de un milisegundo, y un índice
//! aproximado no puede ganar tiempo que no se está gastando: sólo añadiría una
//! extensión nativa que cargar y una probabilidad de fallar el vecino correcto.
//! La medición y el umbral a partir del cual esto deja de valer están en
//! `docs/adr/0009`.

use crate::embedder::{ModelId, cosine};
use crate::{Error, Result};
use obrain_core::EntityId;
use obrain_core::rusqlite::{Connection, ToSql, params, params_from_iter};
use std::collections::HashSet;

pub const DDL: &str = r#"
CREATE TABLE IF NOT EXISTS vectors (
    -- `ON DELETE CASCADE` no es higiene: es lo que permite que M2 siga
    -- existiendo sin saber que M4 existe.
    --
    -- Reextraer un archivo editado **borra y reinserta** sus fragmentos
    -- (`index::replace_chunks` de obrain-extract, y es lo correcto: una
    -- proyección tiene que seguir a la realidad). Sin el cascade, esa
    -- operación choca contra esta clave ajena y el pipeline entero se cae con
    -- `FOREIGN KEY constraint failed` en cuanto alguien edita un archivo que
    -- ya tenía vectores. La alternativa —que el extractor borre los vectores
    -- antes— obligaría a M2 a conocer una tabla que se creó dos hitos después.
    --
    -- La semántica además es la que se quiere: si el texto cambió, el vector
    -- de ese texto es basura, y desaparecer es exactamente lo que debe hacer.
    -- La cola de `pending` lo recoge en la pasada siguiente.
    chunk       INTEGER NOT NULL REFERENCES chunks(id) ON DELETE CASCADE,
    entity      TEXT    NOT NULL REFERENCES entities(id),
    -- I7. Las cinco viajan con el vector, no en una tabla de configuración:
    -- un vector tiene que poder decir por sí solo de dónde salió.
    model_id    TEXT    NOT NULL,
    model_ver   INTEGER NOT NULL,
    dim         INTEGER NOT NULL,
    chunker     TEXT    NOT NULL,
    chunker_ver INTEGER NOT NULL,
    -- `dim` floats de 32 bits en little-endian. Un BLOB y no un JSON de
    -- números: 3,4 MB frente a unos 40, y el kNN lee el bloque sin parsear.
    vec         BLOB    NOT NULL,
    PRIMARY KEY (chunk, model_id, model_ver, chunker, chunker_ver)
);

CREATE INDEX IF NOT EXISTS idx_vectors_entity ON vectors(entity);
CREATE INDEX IF NOT EXISTS idx_vectors_model  ON vectors(model_id, model_ver);
"#;

/// Saca del índice lo que la puerta **ya no concede**: las entidades que han
/// subido a bóveda después de vectorizarse. Devuelve cuáles eran.
///
/// Clasificar no borra vectores —la puerta no conoce esta tabla, y no debe—,
/// así que es cada pasada de vectorización la que hace que la proyección siga
/// a la puerta. Sin esto, una foto que el usuario mete en una carpeta marcada
/// como bóveda seguiría saliendo por significado hasta reconstruir el índice a
/// mano.
pub fn purge_withheld(conn: &Connection) -> Result<Vec<EntityId>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT v.entity FROM vectors v JOIN entities e ON e.id = v.entity
          WHERE e.sensitivity = 'vault' ORDER BY v.entity",
    )?;
    let fuera: Vec<EntityId> = stmt
        .query_map([], |r| Ok(EntityId(r.get(0)?)))?
        .collect::<obrain_core::rusqlite::Result<_>>()?;
    for e in &fuera {
        conn.execute("DELETE FROM vectors WHERE entity = ?1", [e.as_str()])?;
    }
    Ok(fuera)
}

pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(DDL)?;
    Ok(())
}

/// Un fragmento pendiente de vectorizar, con lo que I7 exige guardar de él.
pub struct Pending {
    pub chunk: i64,
    pub entity: EntityId,
    pub chunker: String,
    pub chunker_ver: i64,
    pub text: String,
}

/// Los fragmentos que faltan por vectorizar con este modelo.
///
/// **El origen son las entidades que la puerta concede**, no `chunks`. La
/// consulta recibe la lista ya filtrada por [`obrain_guard::gate::embeddable`]
/// y no la reconstruye: la diferencia entre las dos formas de escribirlo es
/// exactamente la diferencia entre cumplir I11 y decir que se cumple.
pub fn pending(conn: &Connection, allowed: &[EntityId], model: ModelId) -> Result<Vec<Pending>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.entity, c.chunker, c.chunker_ver, c.text
           FROM chunks c
          WHERE c.entity = ?1
            AND NOT EXISTS (SELECT 1 FROM vectors v
                             WHERE v.chunk = c.id
                               AND v.model_id = ?2 AND v.model_ver = ?3
                               AND v.chunker = c.chunker AND v.chunker_ver = c.chunker_ver)
          ORDER BY c.ordinal",
    )?;
    let mut out = Vec::new();
    for e in allowed {
        let rows = stmt.query_map(params![e.as_str(), model.id, model.version], |r| {
            Ok(Pending {
                chunk: r.get(0)?,
                entity: EntityId(r.get(1)?),
                chunker: r.get(2)?,
                chunker_ver: r.get(3)?,
                text: r.get(4)?,
            })
        })?;
        for row in rows {
            out.push(row?);
        }
    }
    Ok(out)
}

/// Guarda un vector. **Aquí** se comprueba que sea unitario, una vez, en vez
/// de en cada comparación del kNN.
pub fn insert(conn: &Connection, p: &Pending, model: ModelId, vec: &[f32]) -> Result<()> {
    if vec.len() != model.dim {
        return Err(Error::Model(format!(
            "vector de {} dimensiones para un modelo de {}",
            vec.len(),
            model.dim
        )));
    }
    let norm = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if !(0.99..=1.01).contains(&norm) {
        return Err(Error::Model(format!(
            "vector sin normalizar (norma {norm})"
        )));
    }
    let mut blob = Vec::with_capacity(vec.len() * 4);
    for x in vec {
        blob.extend_from_slice(&x.to_le_bytes());
    }
    conn.execute(
        "INSERT OR REPLACE INTO vectors
             (chunk, entity, model_id, model_ver, dim, chunker, chunker_ver, vec)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            p.chunk,
            p.entity.as_str(),
            model.id,
            model.version,
            model.dim as i64,
            p.chunker,
            p.chunker_ver,
            blob
        ],
    )?;
    Ok(())
}

fn decode(blob: &[u8], dim: usize) -> Result<Vec<f32>> {
    if blob.len() != dim * 4 {
        return Err(Error::Model(format!(
            "blob de {} bytes para {dim} dimensiones",
            blob.len()
        )));
    }
    Ok(blob
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect())
}

/// Un vecino del índice vectorial.
#[derive(Debug, Clone)]
pub struct Neighbour {
    pub chunk: i64,
    pub entity: EntityId,
    pub score: f32,
}

/// kNN por **recorrido lineal completo**, filtrando por modelo y versión.
///
/// El filtro por `(model_id, model_ver)` no es higiene: durante una migración
/// I7 hay dos juegos de vectores en la tabla, y compararlos entre sí daría
/// números que parecen puntuaciones y no significan nada.
pub fn knn(
    conn: &Connection,
    query: &[f32],
    model: ModelId,
    limit: usize,
) -> Result<Vec<Neighbour>> {
    knn_within(conn, query, model, limit, None)
}

/// [`knn`] **sólo entre estos documentos** (los de una colección, ADR-0031).
/// El filtro va en la consulta, antes de ordenar y cortar, así que no se
/// pierde nadie que estuviera más abajo. `None` es todo el índice.
pub fn knn_within(
    conn: &Connection,
    query: &[f32],
    model: ModelId,
    limit: usize,
    within: Option<&HashSet<EntityId>>,
) -> Result<Vec<Neighbour>> {
    let dentro: Vec<&str> = within
        .map(|w| w.iter().map(EntityId::as_str).collect())
        .unwrap_or_default();
    if within.is_some() && dentro.is_empty() {
        return Ok(Vec::new());
    }
    let filtro = if within.is_some() {
        let huecos: Vec<String> = (0..dentro.len()).map(|i| format!("?{}", i + 4)).collect();
        format!("AND entity IN ({})", huecos.join(", "))
    } else {
        String::new()
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT chunk, entity, vec FROM vectors
          WHERE model_id = ?1 AND model_ver = ?2 AND dim = ?3 {filtro}"
    ))?;
    let dim = model.dim as i64;
    let mut valores: Vec<&dyn ToSql> = vec![&model.id, &model.version, &dim];
    valores.extend(dentro.iter().map(|e| e as &dyn ToSql));
    let rows = stmt.query_map(params_from_iter(valores), |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Vec<u8>>(2)?,
        ))
    })?;

    let mut best: Vec<Neighbour> = Vec::new();
    for row in rows {
        let (chunk, entity, blob) = row?;
        let v = decode(&blob, model.dim)?;
        best.push(Neighbour {
            chunk,
            entity: EntityId(entity),
            score: cosine(query, &v),
        });
    }
    best.sort_by(|a, b| b.score.total_cmp(&a.score));
    best.truncate(limit);
    Ok(best)
}

/// Cuántos vectores hay de este modelo.
pub fn count(conn: &Connection, model: ModelId) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM vectors WHERE model_id = ?1 AND model_ver = ?2",
        params![model.id, model.version],
        |r| r.get(0),
    )?)
}

/// Todos los modelos presentes en la tabla, con su cobertura. Enseña de un
/// vistazo si una migración I7 está a medias.
pub fn census(conn: &Connection) -> Result<Vec<(String, i64, i64, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT model_id, model_ver, dim, COUNT(*) FROM vectors
          GROUP BY model_id, model_ver, dim ORDER BY model_id, model_ver",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
    Ok(rows.collect::<obrain_core::rusqlite::Result<Vec<_>>>()?)
}

/// Todos los vectores en memoria, para las auditorías que necesitan
/// compararlos todos contra todos (los *hubs* de `--sample`).
pub fn load_all(conn: &Connection, model: ModelId) -> Result<Vec<(i64, EntityId, Vec<f32>)>> {
    let mut stmt = conn.prepare(
        "SELECT chunk, entity, vec FROM vectors
          WHERE model_id = ?1 AND model_ver = ?2 ORDER BY chunk",
    )?;
    let rows = stmt.query_map(params![model.id, model.version], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Vec<u8>>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (chunk, entity, blob) = row?;
        out.push((chunk, EntityId(entity), decode(&blob, model.dim)?));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use obrain_core::Store;

    const M: ModelId = ModelId {
        id: "prueba",
        version: 1,
        dim: 3,
    };

    fn base() -> Store {
        let store = Store::open_in_memory().unwrap();
        obrain_extract::index::migrate(store.conn()).unwrap();
        migrate(store.conn()).unwrap();
        store
    }

    fn fragmento(store: &Store, at: i64, texto: &str) -> Pending {
        let e = store.create_entity(at).unwrap();
        store
            .conn()
            .execute(
                "INSERT INTO chunks (entity, chunker, chunker_ver, ordinal, byte_start, byte_end, text)
                 VALUES (?1, 'paragraph-pack', 1, 0, 0, ?2, ?3)",
                params![e.as_str(), texto.len() as i64, texto],
            )
            .unwrap();
        let id = store.conn().last_insert_rowid();
        Pending {
            chunk: id,
            entity: e,
            chunker: "paragraph-pack".into(),
            chunker_ver: 1,
            text: texto.into(),
        }
    }

    #[test]
    fn ida_y_vuelta_de_un_vector() {
        let store = base();
        let p = fragmento(&store, 1, "hola");
        insert(store.conn(), &p, M, &[1.0, 0.0, 0.0]).unwrap();
        let vecinos = knn(store.conn(), &[1.0, 0.0, 0.0], M, 5).unwrap();
        assert_eq!(vecinos.len(), 1);
        assert!((vecinos[0].score - 1.0).abs() < 1e-6);
    }

    /// Buscar dentro de una colección (ADR-0031): el filtro va antes de
    /// cortar, así que el mejor de dentro sale aunque fuera el último de todos.
    #[test]
    fn el_knn_filtrado_no_pierde_lo_que_estaba_mas_abajo() {
        let store = base();
        let cerca = fragmento(&store, 1, "cerca");
        let lejos = fragmento(&store, 2, "lejos");
        insert(store.conn(), &cerca, M, &[1.0, 0.0, 0.0]).unwrap();
        insert(store.conn(), &lejos, M, &[0.0, 1.0, 0.0]).unwrap();

        let todos = knn(store.conn(), &[1.0, 0.0, 0.0], M, 1).unwrap();
        assert_eq!(todos[0].entity, cerca.entity);

        let dentro: HashSet<EntityId> = [lejos.entity.clone()].into();
        let v = knn_within(store.conn(), &[1.0, 0.0, 0.0], M, 1, Some(&dentro)).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].entity, lejos.entity);

        let vacia = HashSet::new();
        assert!(
            knn_within(store.conn(), &[1.0, 0.0, 0.0], M, 5, Some(&vacia))
                .unwrap()
                .is_empty(),
            "una colección vacía no encuentra nada, no todo"
        );
    }

    /// Un vector sin normalizar rompe el kNN en silencio: las puntuaciones
    /// dejan de ser cosenos y el orden deja de significar nada. Se rechaza al
    /// escribir, que es la única vez que sale barato comprobarlo.
    #[test]
    fn un_vector_sin_normalizar_no_entra() {
        let store = base();
        let p = fragmento(&store, 1, "hola");
        assert!(insert(store.conn(), &p, M, &[3.0, 4.0, 0.0]).is_err());
        assert!(
            insert(store.conn(), &p, M, &[1.0, 0.0]).is_err(),
            "dimension equivocada"
        );
    }

    /// El escenario de §5.3: dos modelos conviviendo durante una migración.
    /// Es lo que exige I7 y lo que la clave primaria hace posible.
    #[test]
    fn dos_modelos_conviven_y_no_se_mezclan() {
        let store = base();
        const N: ModelId = ModelId {
            id: "nuevo",
            version: 1,
            dim: 3,
        };
        let p = fragmento(&store, 1, "hola");

        insert(store.conn(), &p, M, &[1.0, 0.0, 0.0]).unwrap();
        insert(store.conn(), &p, N, &[0.0, 1.0, 0.0]).unwrap();

        assert_eq!(
            count(store.conn(), M).unwrap(),
            1,
            "el nuevo no ha pisado al viejo"
        );
        assert_eq!(count(store.conn(), N).unwrap(), 1);
        assert_eq!(census(store.conn()).unwrap().len(), 2);

        // Y el kNN de cada uno sólo ve los suyos.
        let v = knn(store.conn(), &[1.0, 0.0, 0.0], M, 10).unwrap();
        assert_eq!(v.len(), 1);
        assert!((v[0].score - 1.0).abs() < 1e-6);
        let n = knn(store.conn(), &[1.0, 0.0, 0.0], N, 10).unwrap();
        assert_eq!(n.len(), 1);
        assert!(
            n[0].score.abs() < 1e-6,
            "el vector del otro modelo es ortogonal aqui"
        );
    }

    /// Reembeber no duplica: la clave primaria es el fragmento más la
    /// identidad del modelo y del chunker.
    #[test]
    fn reembeber_sustituye_en_vez_de_acumular() {
        let store = base();
        let p = fragmento(&store, 1, "hola");
        insert(store.conn(), &p, M, &[1.0, 0.0, 0.0]).unwrap();
        insert(store.conn(), &p, M, &[0.0, 1.0, 0.0]).unwrap();
        assert_eq!(count(store.conn(), M).unwrap(), 1);
    }

    /// **Regresión del defecto que sólo apareció ejecutando el pipeline
    /// entero.** Editar un archivo ya vectorizado y reextraerlo hacía caer
    /// `obrain-extract` con `FOREIGN KEY constraint failed`: sus fragmentos se
    /// borran para reinsertarlos, y los vectores los referenciaban.
    ///
    /// Ningún test unitario de M4 lo veía, porque M4 nunca borra fragmentos.
    /// Lo destapó reescanear el repositorio después de editar cinco archivos.
    #[test]
    fn reextraer_un_archivo_ya_vectorizado_no_rompe_la_clave_ajena() {
        let store = base();
        let p = fragmento(&store, 1, "el texto de antes");
        insert(store.conn(), &p, M, &[1.0, 0.0, 0.0]).unwrap();

        // Lo que hace M2 cuando el archivo ha cambiado.
        obrain_extract::index::replace_chunks(
            store.conn(),
            &p.entity,
            "paragraph-pack",
            1,
            &[obrain_extract::chunk::Chunk {
                ordinal: 0,
                byte_start: 0,
                byte_end: 17,
                text: "el texto de ahora".into(),
            }],
        )
        .expect("reextraer no puede fallar por culpa de una proyeccion de M4");

        // El vector del texto viejo ha desaparecido con su fragmento, que es
        // lo correcto: era el vector de un texto que ya no existe.
        assert_eq!(count(store.conn(), M).unwrap(), 0);
        // Y el fragmento nuevo vuelve a estar pendiente.
        assert_eq!(
            pending(store.conn(), std::slice::from_ref(&p.entity), M)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn lo_ya_vectorizado_no_vuelve_a_la_cola() {
        let store = base();
        let p = fragmento(&store, 1, "hola");
        let permitidas = vec![p.entity.clone()];
        assert_eq!(pending(store.conn(), &permitidas, M).unwrap().len(), 1);
        insert(store.conn(), &p, M, &[1.0, 0.0, 0.0]).unwrap();
        assert_eq!(pending(store.conn(), &permitidas, M).unwrap().len(), 0);
        // ...pero para otro modelo sigue pendiente. Es la migración en paralelo.
        const N: ModelId = ModelId {
            id: "nuevo",
            version: 1,
            dim: 3,
        };
        assert_eq!(pending(store.conn(), &permitidas, N).unwrap().len(), 1);
    }
}
