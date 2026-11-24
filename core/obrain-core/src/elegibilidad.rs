//! **Qué puede salir en una búsqueda**, escrito una sola vez y en SQL.
//!
//! Las dos ramas de recuperación (BM25 en `obrain-extract`, kNN en
//! `obrain-embed`) cortan su lista en algún momento: `LIMIT` en una, `truncate`
//! en la otra. Todo lo que se quite **después** de cortar deja huecos que nadie
//! rellena: si los diez primeros fragmentos de una consulta son de la papelera,
//! el undécimo, vigente, nunca sale, y la lista se queda vacía teniendo
//! respuesta (`PLAN-CONFIANZA.md` punto 3, ADR-0038 «Dónde va en el plan»).
//!
//! Por eso la elegibilidad no es un filtro de Rust sobre los resultados, sino
//! un trozo del `WHERE` que cada rama pega en su consulta, antes de ordenar y
//! cortar. Y está aquí, en el núcleo, y no copiada en cada rama: dos
//! definiciones de «lo que puede salir» acaban divergiendo (ADR-0012).
//!
//! El trozo usa los parámetros con nombre `:valid` y `:known`, los dos ejes de
//! [`crate::TimePoint`]: la ruta vigente y la identidad pendiente son
//! aserciones bitemporales, y se leen igual que las lee [`crate::Store`].

use crate::store::{BITEMPORAL, PRECEDENCE};

/// Quién va a ver la lista, y por tanto qué puede entrar en ella.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Elegibles {
    /// **La ventana de Brain**, donde mira JP. Entra todo lo que el
    /// explorador enseña: lo publicado (o lo que nunca tuvo cuarentena, como
    /// una carpeta), con ruta vigente y sin identidad pendiente. Lo sensible y
    /// lo de bóveda entran con su nombre; qué texto se enseña de ellos lo
    /// decide después la superficie según el PIN (`contenido_visible`).
    Ventana,
    /// **Lo que puede cruzar la puerta de Aetheris** (ADR-0038, B5 y ajuste
    /// 3): sólo `sensitivity = 'normal'`, con la publicación en `ready`
    /// —clasificado en su versión actual; sin fila cuenta como no
    /// clasificado—, ruta vigente e identidad confirmada. No depende del PIN
    /// ni de la ventana: por la puerta no mira JP.
    SoloNormal,
}

/// Expresión SQL con la **ruta vigente** de la entidad `entity` en
/// `(:valid, :known)`, o `NULL` si no la tiene (está en la papelera, ADR-0025,
/// o su ruta se retiró por identidad pendiente, ADR-0036).
///
/// Es exactamente `Store::resolve(entity, "path_observed_at", at)` seguido de
/// `as_text()`: la misma condición bitemporal, la misma precedencia (I5) y el
/// mismo «sólo si la aserción ganadora es texto». Tiene que serlo: si esta
/// expresión y `resolve` discreparan, la búsqueda enseñaría rutas que el resto
/// de la aplicación no ve, o al revés.
///
/// `entity` es una columna escrita por el llamante, **siempre con su tabla**
/// (`c.entity`, `vectors.entity`), nunca un valor que venga de fuera. Sin
/// tabla no vale: dentro de `content_publication p`, un `entity` suelto es
/// `p.entity`, y `p.entity = entity` se cumple siempre. Las pruebas de las dos
/// ramas lo cazaron así: lo pendiente de un documento escondía a todos.
pub fn ruta_vigente_sql(entity: &str) -> String {
    con_tabla(entity);
    format!(
        "(SELECT CASE WHEN object_kind = 'text' THEN object_text END
            FROM assertions
           WHERE subject = {entity} AND predicate = 'path_observed_at' AND {BITEMPORAL}
           {PRECEDENCE} LIMIT 1)"
    )
}

/// Falla en el acto si la columna llega sin su tabla (ver
/// [`ruta_vigente_sql`]). Las columnas las escribe el código, así que un fallo
/// aquí es un error de programación y sale en la primera prueba que lo pise.
fn con_tabla(entity: &str) {
    assert!(
        entity.contains('.'),
        "la columna de la entidad tiene que llevar su tabla: `{entity}`"
    );
}

impl Elegibles {
    /// La condición, lista para ir tras un `AND` en el `WHERE` de una rama.
    /// `entity` como en [`ruta_vigente_sql`].
    pub fn sql(self, entity: &str) -> String {
        con_tabla(entity);
        // Se comprueba la marca positiva de identidad pendiente y no sólo la
        // falta de ruta (ADR-0036): hoy van juntas, pero la marca es la que
        // dice por qué no se enseña.
        let comun = format!(
            "{ruta} IS NOT NULL
             AND NOT EXISTS (SELECT 1 FROM assertions
                              WHERE subject = {entity}
                                AND predicate = 'identity_pending_path'
                                AND {BITEMPORAL})",
            ruta = ruta_vigente_sql(entity),
        );
        match self {
            Elegibles::Ventana => format!(
                "NOT EXISTS (SELECT 1 FROM content_publication p
                              WHERE p.entity = {entity} AND p.state = 'pending')
                 AND {comun}"
            ),
            Elegibles::SoloNormal => format!(
                "EXISTS (SELECT 1 FROM entities e
                          WHERE e.id = {entity} AND e.sensitivity = 'normal')
                 AND EXISTS (SELECT 1 FROM content_publication p
                              WHERE p.entity = {entity} AND p.state = 'ready')
                 AND {comun}"
            ),
        }
    }
}

/// `AND {entity} IN (:d0, :d1, …)` para buscar sólo entre estos documentos
/// (una colección, ADR-0031), o nada si `dentro` es `None`. Los valores se
/// enlazan con los nombres de [`nombre_dentro`]; nunca se pegan en el texto.
///
/// Con `Some` de una lista vacía no hay consulta que hacer: el llamante
/// devuelve vacío antes (una colección vacía no encuentra nada, no todo).
pub fn dentro_sql(entity: &str, dentro: Option<usize>) -> String {
    match dentro {
        Some(n) => {
            let huecos: Vec<String> = (0..n).map(nombre_dentro).collect();
            format!("AND {entity} IN ({})", huecos.join(", "))
        }
        None => String::new(),
    }
}

/// El nombre del parámetro del `i`-ésimo documento de [`dentro_sql`].
pub fn nombre_dentro(i: usize) -> String {
    format!(":d{i}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Method, NewAssertion, Store, TimePoint, Value};
    use rusqlite::named_params;

    fn ruta(store: &Store, e: &crate::EntityId, at: TimePoint) -> Option<String> {
        store
            .conn()
            .query_row(
                &format!(
                    "SELECT {} FROM entities x WHERE x.id = :e",
                    ruta_vigente_sql("x.id")
                ),
                named_params! {":e": e.as_str(), ":valid": at.valid, ":known": at.known},
                |r| r.get(0),
            )
            .unwrap()
    }

    fn afirmar(store: &Store, e: &crate::EntityId, ruta: &str, method: Method, t: i64) {
        store
            .assert(NewAssertion {
                subject: e.clone(),
                predicate: "path_observed_at".into(),
                object: Value::Text(ruta.into()),
                valid_from: Some(t),
                valid_to: None,
                method,
                source: "prueba".into(),
                confidence: 1.0,
                evidence: None,
                asserted_at: t,
            })
            .unwrap();
    }

    /// La expresión SQL y `Store::resolve` dicen lo mismo en los casos que
    /// importan: sin ruta, con ruta, con dos rutas y precedencia del usuario,
    /// después de retractar y antes de que existiera.
    #[test]
    fn la_ruta_vigente_en_sql_es_la_de_resolve() {
        let store = Store::open_in_memory().unwrap();
        let e = store.create_entity(1).unwrap();
        let comparar = |at: TimePoint| {
            let esperado = store
                .resolve(&e, "path_observed_at", at)
                .unwrap()
                .and_then(|a| a.object.as_text().map(str::to_string));
            assert_eq!(ruta(&store, &e, at), esperado, "en {at:?}");
        };
        comparar(TimePoint::at(5));
        afirmar(&store, &e, "C:/a.txt", Method::Deterministic, 10);
        comparar(TimePoint::at(5));
        comparar(TimePoint::at(10));
        afirmar(&store, &e, "C:/b.txt", Method::User, 20);
        afirmar(&store, &e, "C:/c.txt", Method::Deterministic, 30);
        comparar(TimePoint::at(25));
        comparar(TimePoint::at(35));
        assert_eq!(
            ruta(&store, &e, TimePoint::at(35)).as_deref(),
            Some("C:/b.txt")
        );
        for a in store
            .query(Some(&e), Some("path_observed_at"), TimePoint::at(40))
            .unwrap()
        {
            store.retract(&a.id, 40).unwrap();
        }
        comparar(TimePoint::at(45));
        assert_eq!(ruta(&store, &e, TimePoint::at(45)), None);
        comparar(TimePoint::as_known_at(35, 35));
    }

    #[test]
    #[should_panic(expected = "tiene que llevar su tabla")]
    fn una_columna_sin_tabla_no_se_acepta() {
        Elegibles::Ventana.sql("entity");
    }
}
