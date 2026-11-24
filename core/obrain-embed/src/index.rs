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
use obrain_core::rusqlite::{Connection, ToSql, params};
use obrain_core::{Elegibles, EntityId, TimePoint, elegibilidad};
use std::collections::{HashMap, HashSet};

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
/// Un vecino por fragmento, sin mirar papelera ni identidad: es el índice en
/// crudo, para medirlo. Lo que se enseña sale de [`knn_documents`].
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
    let mut best = scan(conn, query, model, "", &[])?;
    best.sort_by(|a, b| b.score.total_cmp(&a.score));
    best.truncate(limit);
    Ok(best)
}

/// Los `limit` **documentos** más cercanos, cada uno con su mejor fragmento,
/// entre los elegibles ([`Elegibles`]) en `at` y, si hay `within`, sólo entre
/// ésos (los de una colección, ADR-0031; `None` es todo el índice).
///
/// Todos los filtros van en la consulta y la deduplicación por documento va
/// antes de cortar. Antes se pedían 200 fragmentos y la papelera y los
/// repetidos se quitaban después: con un documento largo cuyos 200 fragmentos
/// se parecían a la consulta, o con lo de arriba en la papelera, el documento
/// vigente que venía detrás no salía nunca (`PLAN-CONFIANZA.md` punto 3).
pub fn knn_documents(
    conn: &Connection,
    query: &[f32],
    model: ModelId,
    limit: usize,
    at: TimePoint,
    within: Option<&HashSet<EntityId>>,
    elegibles: Elegibles,
) -> Result<Vec<Neighbour>> {
    let dentro: Vec<&str> = within
        .map(|w| w.iter().map(EntityId::as_str).collect())
        .unwrap_or_default();
    if limit == 0 || (within.is_some() && dentro.is_empty()) {
        return Ok(Vec::new());
    }
    let filtro = format!(
        "AND {} {}",
        elegibles.sql("vectors.entity"),
        elegibilidad::dentro_sql("vectors.entity", within.map(|_| dentro.len())),
    );
    let nombres: Vec<String> = (0..dentro.len()).map(elegibilidad::nombre_dentro).collect();
    let mut valores: Vec<(&str, &dyn ToSql)> = vec![(":valid", &at.valid), (":known", &at.known)];
    valores.extend(
        nombres
            .iter()
            .map(String::as_str)
            .zip(dentro.iter().map(|e| e as &dyn ToSql)),
    );

    // El mejor fragmento de cada documento; a igualdad, el primero que se
    // leyó, para que el resultado no dependa del orden del HashMap.
    let mut por_documento: HashMap<EntityId, Neighbour> = HashMap::new();
    for n in scan(conn, query, model, &filtro, &valores)? {
        match por_documento.get(&n.entity) {
            Some(ya) if ya.score >= n.score => {}
            _ => {
                por_documento.insert(n.entity.clone(), n);
            }
        }
    }
    let mut best: Vec<Neighbour> = por_documento.into_values().collect();
    best.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.entity.as_str().cmp(b.entity.as_str()))
    });
    best.truncate(limit);
    Ok(best)
}

/// El recorrido: todos los vectores del modelo que pasan el filtro, con su
/// coseno contra la consulta. Lo de bóveda y lo pendiente de publicar no
/// entran nunca, aunque la purga no haya ocurrido; `filtro` añade el resto
/// con sus parámetros con nombre.
fn scan(
    conn: &Connection,
    query: &[f32],
    model: ModelId,
    filtro: &str,
    extra: &[(&str, &dyn ToSql)],
) -> Result<Vec<Neighbour>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT chunk, entity, vec FROM vectors
          WHERE model_id = :model_id AND model_ver = :model_ver AND dim = :dim
          AND entity IN (SELECT id FROM entities WHERE sensitivity != 'vault')
          AND NOT EXISTS (SELECT 1 FROM content_publication p
                           WHERE p.entity=vectors.entity AND p.state='pending') {filtro}"
    ))?;
    let dim = model.dim as i64;
    let mut valores: Vec<(&str, &dyn ToSql)> = vec![
        (":model_id", &model.id),
        (":model_ver", &model.version),
        (":dim", &dim),
    ];
    valores.extend_from_slice(extra);
    let rows = stmt.query_map(valores.as_slice(), |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Vec<u8>>(2)?,
        ))
    })?;

    let mut out: Vec<Neighbour> = Vec::new();
    for row in rows {
        let (chunk, entity, blob) = row?;
        let v = decode(&blob, model.dim)?;
        out.push(Neighbour {
            chunk,
            entity: EntityId(entity),
            score: cosine(query, &v),
        });
    }
    Ok(out)
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

    #[test]
    fn el_knn_no_usa_vectores_vault_aunque_la_purga_no_haya_ocurrido() {
        let store = base();
        let p = fragmento(&store, 1, "SINTETICO");
        insert(store.conn(), &p, M, &[1.0, 0.0, 0.0]).unwrap();
        assert_eq!(knn(store.conn(), &[1.0, 0.0, 0.0], M, 5).unwrap().len(), 1);
        // Residuo histórico sintético: omite a propósito el setter seguro.
        store
            .conn()
            .execute(
                "UPDATE entities SET sensitivity = 'vault' WHERE id = ?1",
                [p.entity.as_str()],
            )
            .unwrap();
        assert_eq!(count(store.conn(), M).unwrap(), 1);
        assert!(
            knn(store.conn(), &[1.0, 0.0, 0.0], M, 5)
                .unwrap()
                .is_empty()
        );
    }

    /// Le da a la entidad una ruta vigente desde `t`: lo que tiene un
    /// documento que no está en la papelera.
    fn con_ruta(store: &Store, p: &Pending, t: i64) {
        store
            .assert(obrain_core::NewAssertion {
                subject: p.entity.clone(),
                predicate: "path_observed_at".into(),
                object: obrain_core::Value::Text(format!("C:/sintetica/{}.txt", p.entity)),
                valid_from: Some(t),
                valid_to: None,
                method: obrain_core::Method::Deterministic,
                source: "prueba".into(),
                confidence: 1.0,
                evidence: None,
                asserted_at: t,
            })
            .unwrap();
    }

    const AHORA: TimePoint = TimePoint {
        valid: 100,
        known: 100,
    };

    fn documentos(
        store: &Store,
        limit: usize,
        within: Option<&HashSet<EntityId>>,
    ) -> Vec<Neighbour> {
        knn_documents(
            store.conn(),
            &[1.0, 0.0, 0.0],
            M,
            limit,
            AHORA,
            within,
            Elegibles::Ventana,
        )
        .unwrap()
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
        con_ruta(&store, &cerca, 1);
        con_ruta(&store, &lejos, 2);

        let todos = documentos(&store, 1, None);
        assert_eq!(todos[0].entity, cerca.entity);

        let dentro: HashSet<EntityId> = [lejos.entity.clone()].into();
        let v = documentos(&store, 1, Some(&dentro));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].entity, lejos.entity);

        let vacia = HashSet::new();
        assert!(
            documentos(&store, 5, Some(&vacia)).is_empty(),
            "una colección vacía no encuentra nada, no todo"
        );
    }

    /// Un documento cuyos fragmentos son todos más parecidos a la consulta que
    /// el siguiente documento ocupa **un** puesto, no todos: la deduplicación
    /// va antes de cortar. Antes se pedían 200 fragmentos y se deduplicaba
    /// después, y con 250 fragmentos del mismo documento el segundo no salía.
    #[test]
    fn un_documento_largo_no_tapa_al_siguiente() {
        let store = base();
        let largo = fragmento(&store, 1, "largo 0");
        con_ruta(&store, &largo, 1);
        insert(store.conn(), &largo, M, &[1.0, 0.0, 0.0]).unwrap();
        for i in 1..250 {
            let texto = format!("largo {i}");
            store
                .conn()
                .execute(
                    "INSERT INTO chunks (entity, chunker, chunker_ver, ordinal, byte_start, byte_end, text)
                     VALUES (?1, 'paragraph-pack', 1, ?2, 0, ?3, ?4)",
                    params![largo.entity.as_str(), i, texto.len() as i64, texto],
                )
                .unwrap();
            let p = Pending {
                chunk: store.conn().last_insert_rowid(),
                text: texto,
                ..Pending {
                    chunk: 0,
                    entity: largo.entity.clone(),
                    chunker: "paragraph-pack".into(),
                    chunker_ver: 1,
                    text: String::new(),
                }
            };
            insert(store.conn(), &p, M, &[1.0, 0.0, 0.0]).unwrap();
        }
        let otro = fragmento(&store, 2, "otro");
        con_ruta(&store, &otro, 2);
        insert(store.conn(), &otro, M, &[0.6, 0.8, 0.0]).unwrap();

        let v = documentos(&store, 2, None);
        let entidades: Vec<&EntityId> = v.iter().map(|n| &n.entity).collect();
        assert_eq!(entidades, vec![&largo.entity, &otro.entity]);
    }

    /// Lo de la papelera (sin ruta vigente) y lo de identidad pendiente no
    /// salen ni ocupan puesto: el siguiente vigente sube.
    #[test]
    fn la_papelera_y_la_identidad_pendiente_no_ocupan_puesto() {
        let store = base();
        let papelera = fragmento(&store, 1, "papelera");
        let pendiente = fragmento(&store, 2, "pendiente");
        let vigente = fragmento(&store, 3, "vigente");
        insert(store.conn(), &papelera, M, &[1.0, 0.0, 0.0]).unwrap();
        insert(store.conn(), &pendiente, M, &[1.0, 0.0, 0.0]).unwrap();
        insert(store.conn(), &vigente, M, &[0.6, 0.8, 0.0]).unwrap();
        con_ruta(&store, &papelera, 1);
        con_ruta(&store, &pendiente, 2);
        con_ruta(&store, &vigente, 3);
        // A la papelera: su ruta se retracta (ADR-0025).
        for a in store
            .query(Some(&papelera.entity), Some("path_observed_at"), AHORA)
            .unwrap()
        {
            store.retract(&a.id, 50).unwrap();
        }
        // Identidad pendiente: la marca positiva (ADR-0036).
        store
            .assert(obrain_core::NewAssertion {
                subject: pendiente.entity.clone(),
                predicate: "identity_pending_path".into(),
                object: obrain_core::Value::Text("C:/sintetica/pendiente.txt".into()),
                valid_from: Some(50),
                valid_to: None,
                method: obrain_core::Method::Deterministic,
                source: "prueba".into(),
                confidence: 1.0,
                evidence: None,
                asserted_at: 50,
            })
            .unwrap();

        let v = documentos(&store, 1, None);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].entity, vigente.entity);
    }

    /// Para la puerta (ADR-0038): diez sensibles más cerca que el normal no
    /// lo tapan, y lo que no está publicado en `ready` no sale.
    #[test]
    fn solo_normal_devuelve_el_normal_que_venia_detras() {
        let store = base();
        for t in 1..=10 {
            let s = fragmento(&store, t, "sensible");
            con_ruta(&store, &s, t);
            insert(store.conn(), &s, M, &[1.0, 0.0, 0.0]).unwrap();
            store
                .conn()
                .execute(
                    "UPDATE entities SET sensitivity = 'sensitive' WHERE id = ?1",
                    [s.entity.as_str()],
                )
                .unwrap();
            store.prepare_content(&s.entity, None).unwrap();
            store.publish_content(&s.entity, None).unwrap();
        }
        let sin_fila = fragmento(&store, 20, "sin publicar");
        con_ruta(&store, &sin_fila, 20);
        insert(store.conn(), &sin_fila, M, &[0.9, 0.0, 0.43589]).unwrap();
        let normal = fragmento(&store, 21, "normal");
        con_ruta(&store, &normal, 21);
        insert(store.conn(), &normal, M, &[0.6, 0.8, 0.0]).unwrap();
        store.prepare_content(&normal.entity, None).unwrap();
        store.publish_content(&normal.entity, None).unwrap();

        let v = knn_documents(
            store.conn(),
            &[1.0, 0.0, 0.0],
            M,
            5,
            AHORA,
            None,
            Elegibles::SoloNormal,
        )
        .unwrap();
        let entidades: Vec<&EntityId> = v.iter().map(|n| &n.entity).collect();
        assert_eq!(entidades, vec![&normal.entity]);

        // La ventana sí ve los sensibles y lo que nunca tuvo cuarentena.
        assert_eq!(documentos(&store, 20, None).len(), 12);
    }

    /// Para la puerta (ADR-0039, condición 1): en kNN tampoco sale la copia
    /// sin marcar de algo protegido, aunque sea lo más cercano.
    #[test]
    fn solo_normal_no_deja_salir_una_copia_de_algo_protegido() {
        let store = base();
        let publicado = |p: &Pending, t: i64, v: &[f32]| {
            con_ruta(&store, p, t);
            insert(store.conn(), p, M, v).unwrap();
            store.prepare_content(&p.entity, None).unwrap();
            store.publish_content(&p.entity, None).unwrap();
            store
                .conn()
                .execute(
                    "INSERT INTO extractions (entity, extractor, extractor_ver, chunker, chunker_ver,
                                              status, text_hash, chars, extracted_at)
                     VALUES (?1, 'txt', 1, 'paragraph-pack', 1, 'ok', ?2, 20, 1)",
                    params![p.entity.as_str(), format!("hash-{}", p.text)],
                )
                .unwrap();
        };
        let original = fragmento(&store, 1, "pasaporte");
        let copia = fragmento(&store, 2, "pasaporte");
        let otro = fragmento(&store, 3, "recibo");
        publicado(&original, 1, &[1.0, 0.0, 0.0]);
        publicado(&copia, 2, &[1.0, 0.0, 0.0]);
        publicado(&otro, 3, &[0.6, 0.8, 0.0]);
        store
            .conn()
            .execute(
                "UPDATE entities SET sensitivity = 'vault' WHERE id = ?1",
                [original.entity.as_str()],
            )
            .unwrap();

        let v = knn_documents(
            store.conn(),
            &[1.0, 0.0, 0.0],
            M,
            5,
            AHORA,
            None,
            Elegibles::SoloNormal,
        )
        .unwrap();
        let entidades: Vec<&EntityId> = v.iter().map(|n| &n.entity).collect();
        assert_eq!(entidades, vec![&otro.entity]);
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
