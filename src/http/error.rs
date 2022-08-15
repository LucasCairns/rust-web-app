use axum::{response::IntoResponse, Json};
use hyper::StatusCode;
use serde::Serialize;
use serde_with::DisplayFromStr;
use validator::ValidationErrors;

#[derive(thiserror::Error, Debug)]
pub enum ApiError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("An error occurred whilst querying the database")]
    DatabaseError(#[from] sqlx::Error),
    #[error("Invalid request")]
    ValidationError(#[from] ValidationErrors),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        #[serde_with::serde_as]
        #[serde_with::skip_serializing_none]
        #[derive(Serialize)]
        struct ErrorResponse<'a> {
            #[serde_as(as = "DisplayFromStr")]
            message: &'a ApiError,
            errors: Option<&'a ValidationErrors>,
        }

        let validation_errors = match &self {
            ApiError::ValidationError(e) => Some(e),
            _ => None,
        };

        (
            self.status_code(),
            Json(ErrorResponse {
                message: &self,
                errors: validation_errors,
            }),
        )
            .into_response()
    }
}

impl ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            ApiError::DatabaseError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::ValidationError(_) => StatusCode::BAD_REQUEST,
        }
    }
}
