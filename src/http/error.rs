use axum::response::IntoResponse;
use hyper::StatusCode;
use serde::Serialize;
use serde_with::DisplayFromStr;
use utoipa::ToSchema;
use validator::ValidationErrors;

#[derive(thiserror::Error, Debug)]
pub enum ApiError {
    #[error("Invalid request")]
    ValidationError(#[from] ValidationErrors),
    #[error("Database error")]
    DatabaseError(#[from] sqlx::Error),
}

#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Serialize, ToSchema)]
pub struct ErrorResponse<'a> {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type=String)]
    message: &'a ApiError,
    #[schema(value_type=Option<& ()>)]
    errors: Option<&'a ValidationErrors>,
}

const POSTGRES_UNIQUE_VIOLATION: &str = "23505";

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        match &self {
            ApiError::ValidationError(validation_errors) => (
                StatusCode::BAD_REQUEST,
                format!("Validation Errors: {:?}", validation_errors),
            ),
            ApiError::DatabaseError(db_error) => match db_error {
                sqlx::Error::RowNotFound => {
                    (StatusCode::NOT_FOUND, "Resource not found".to_string())
                }
                sqlx::Error::Database(ref dbe) => match dbe.code().as_deref() {
                    Some(POSTGRES_UNIQUE_VIOLATION) => {
                        (StatusCode::CONFLICT, "Duplicate entry".to_string())
                    }
                    _ => {
                        tracing::error!("Database error: {:?}", self);
                        (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Something went wrong".to_string(),
                    )
                    
                },
                },
                sqlx::Error::Io(ioe) if ioe.kind() == std::io::ErrorKind::TimedOut => (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Database timeout".to_string(),
                ),
                _ => {
                    tracing::error!("Database error: {:?}", self);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Something went wrong".to_string(),
                    )
                }
            },
        }
        .into_response()
    }
}
