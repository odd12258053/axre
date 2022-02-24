use ntex::http::StatusCode;
use ntex::web::error::PayloadError;
use ntex::web::WebResponseError;
// use ntex::web::error::JsonPayloadError;

#[derive(thiserror::Error, Debug)]
pub enum JsonPayloadError {
    /// Payload size is bigger than allowed. (default: 32kB)
    #[error("Json payload size is bigger than allowed")]
    Overflow,
    /// Content type error
    #[error("Content type error")]
    ContentType,
    /// Deserialize error
    #[error("Json deserialize error: {0}")]
    Deserialize(#[from] dade::Error),
    /// Payload error
    #[error("Error that occur during reading payload: {0}")]
    Payload(#[from] PayloadError),
}

impl From<ntex::http::error::PayloadError> for JsonPayloadError {
    fn from(err: ntex::http::error::PayloadError) -> Self {
        JsonPayloadError::Payload(err.into())
    }
}

impl WebResponseError for JsonPayloadError {
    fn status_code(&self) -> StatusCode {
        match self {
            JsonPayloadError::Overflow => StatusCode::INTERNAL_SERVER_ERROR,
            JsonPayloadError::ContentType => StatusCode::BAD_REQUEST,
            JsonPayloadError::Deserialize(_) => StatusCode::BAD_REQUEST,
            JsonPayloadError::Payload(_) => StatusCode::BAD_REQUEST,
        }
    }
}
