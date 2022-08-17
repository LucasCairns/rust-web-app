use std::env;

use axum::{
    async_trait,
    extract::{FromRequest, RequestParts},
    headers::{authorization::Bearer, Authorization},
    response::{IntoResponse, Response},
    Json, TypedHeader,
};
use hyper::StatusCode;
use jsonwebtoken::{
    decode, decode_header,
    jwk::{AlgorithmParameters, JwkSet},
    DecodingKey, Validation,
};
use serde::Deserialize;
use serde_json::json;

pub enum AuthError {
    InvalidToken,
    ExpiredToken,
    Unavailable,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "Invalid token"),
            AuthError::ExpiredToken => (StatusCode::UNAUTHORIZED, "Token expired"),
            AuthError::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "Unable to verify JWT token",
            ),
        };
        let body = Json(json!({
            "error": error_message,
        }));
        (status, body).into_response()
    }
}

async fn get_jwks() -> Result<JwkSet, AuthError> {
    let auth_url = env::var("AUTH_URL").map_err(|_| AuthError::Unavailable)?;

    reqwest::get(format!("{auth_url}/.well-known/jwks.json"))
        .await
        .map_err(|_| AuthError::Unavailable)?
        .json::<JwkSet>()
        .await
        .map_err(|_| AuthError::Unavailable)
}

impl From<jsonwebtoken::errors::Error> for AuthError {
    fn from(error: jsonwebtoken::errors::Error) -> Self {
        match error.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::ExpiredToken,
            _ => AuthError::InvalidToken,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Claims {
    iss: String,
    sub: String,
    exp: usize,
    scope: Vec<String>,
    authorities: Vec<String>,
}

#[async_trait]
impl<B> FromRequest<B> for Claims
where
    B: Send,
{
    type Rejection = AuthError;

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        let TypedHeader(Authorization(bearer_token)) =
            TypedHeader::<Authorization<Bearer>>::from_request(req)
                .await
                .map_err(|_| AuthError::InvalidToken)?;

        let header = decode_header(bearer_token.token())?;

        let kid = match header.kid {
            Some(k) => k,
            None => return Err(AuthError::InvalidToken),
        };

        let jwks = get_jwks().await?;

        let decoded =  match jwks.find(&kid) {
            Some(j) => match j.algorithm {
                AlgorithmParameters::RSA(ref rsa) => {
                    let decoding_key = DecodingKey::from_rsa_components(&rsa.n, &rsa.e).unwrap();
                    let validation = Validation::new(j.common.algorithm.unwrap());

                    decode::<Claims>(bearer_token.token(), &decoding_key, &validation).map_err(AuthError::from)
                }
                _ => Err(AuthError::InvalidToken),
            },
            None => Err(AuthError::InvalidToken),
        }?;

        Ok(decoded.claims)
    }
}
