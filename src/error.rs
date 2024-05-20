pub type Result<T> = core::result::Result<T, self::Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error:\n{0}")]
    IOError(#[from] std::io::Error),
    #[error("Connection to internal database is broken")]
    StoreConnectionBroken,
    #[error("Bad metadata processing")]
    BadMetadataProcessing,
}

impl actix_web::ResponseError for Error {
    fn status_code(&self) -> actix_web::http::StatusCode {
        actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
    }
}
