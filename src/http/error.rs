use axum::{response::IntoResponse, Json};
use hyper::StatusCode;
use serde::Serialize;
use utoipa::ToSchema;
use validator::ValidationErrors;

use crate::http::auth::AuthError;

#[derive(thiserror::Error, Debug)]
pub enum ApiError {
    #[error("Invalid request")]
    ValidationError(#[from] ValidationErrors),
    #[error("Database error")]
    DatabaseError(#[from] sqlx::Error),
    #[error("Auth error")]
    AuthError(#[from] AuthError),
}

#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub errors: Option<serde_json::Value>,
}

const POSTGRES_UNIQUE_VIOLATION: &str = "23505";

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, message, errors) = match &self {
            ApiError::ValidationError(validation_errors) => {
                (StatusCode::BAD_REQUEST, self.to_string(), serde_json::to_value(validation_errors).ok())
            }
            ApiError::DatabaseError(db_error) => {
                let (status, message) = match db_error {
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
                        }
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
                };
                (status, message, None)
            }
            ApiError::AuthError(auth_err) => {
                let (status, message) = match auth_err {
                    AuthError::MissingToken => (StatusCode::UNAUTHORIZED, auth_err.to_string()),
                    AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, auth_err.to_string()),
                    AuthError::ExpiredToken => (StatusCode::UNAUTHORIZED, auth_err.to_string()),
                    AuthError::Unavailable => {
                        (StatusCode::SERVICE_UNAVAILABLE, auth_err.to_string())
                    }
                    AuthError::MissingScope(scope) => (
                        StatusCode::FORBIDDEN,
                        format!("Client requires the scope: {scope}"),
                    ),
                };
                (status, message, None)
            }
        };
        (status, Json(ErrorResponse { message, errors })).into_response()
    }
}
