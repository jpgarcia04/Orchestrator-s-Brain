use thiserror::Error as ThisError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, ThisError)]
pub enum Error {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("entidad desconocida: {0}")]
    UnknownEntity(String),

    #[error("predicado no registrado: {0} (el vocabulario es controlado)")]
    UnknownPredicate(String),

    #[error("asercion desconocida: {0}")]
    UnknownAssertion(String),

    #[error("dato corrupto en el log: {0}")]
    Corrupt(String),
}

impl Error {
    /// ¿Es este error el rechazo de un trigger que protege un invariante?
    /// Los tests lo usan para distinguir "la escritura falló" de
    /// "el motor impidió violar el modelo".
    pub fn is_invariant_violation(&self) -> bool {
        match self {
            Error::Sqlite(rusqlite::Error::SqliteFailure(_, Some(msg))) => {
                msg.starts_with("I1:") || msg.starts_with("I4:")
            }
            _ => false,
        }
    }
}
