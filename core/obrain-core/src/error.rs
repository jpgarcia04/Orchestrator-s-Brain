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

    #[error("no es una carpeta: {0}")]
    NotAFolder(String),

    #[error("una carpeta se marca como sensible o de bóveda, no como {0}")]
    InvalidMark(String),

    /// La llave de la base no se pudo abrir (ADR-0021).
    #[error(transparent)]
    Key(#[from] obrain_keys::Error),

    /// La base no se abre con esta llave: o es otra base, o la llave es de otra.
    #[error("la base no se abre con esta llave")]
    WrongKey,
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
