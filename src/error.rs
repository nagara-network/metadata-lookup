pub type Result<T> = core::result::Result<T, self::Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error")]
    IOError(#[from] std::io::Error),
    #[error("Subxt error")]
    SubxtError(#[from] subxt::Error),
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
