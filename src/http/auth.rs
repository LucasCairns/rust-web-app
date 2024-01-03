use axum::{
    async_trait,
    extract::FromRequestParts,
    http::request::Parts,
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::{
    headers::{authorization::Bearer, Authorization},
    TypedHeader,
};
use hyper::StatusCode;
use jsonwebtoken::{
    decode, decode_header,
    jwk::{AlgorithmParameters, JwkSet},
    Algorithm, DecodingKey, Validation,
};
use serde::Deserialize;
use serde_json::json;
use std::{env, str::FromStr};

pub enum AuthError {
    MissingToken,
    InvalidToken,
    ExpiredToken,
    Unavailable,
    MissingScope(String),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AuthError::MissingToken => (StatusCode::UNAUTHORIZED, "Missing token".to_owned()),
            AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "Invalid token".to_owned()),
            AuthError::ExpiredToken => (StatusCode::UNAUTHORIZED, "Token expired".to_owned()),
            AuthError::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "Unable to verify JWT token".to_owned(),
            ),
            AuthError::MissingScope(scope) => (
                StatusCode::FORBIDDEN,
                format!("Client requires the scope: {}", scope),
            ),
        };
        let body = Json(json!({
            "message": error_message,
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
#[derive(Clone, Debug, Deserialize)]
pub struct Claims {
    iss: String,
    sub: String,
    exp: usize,
    scope: Vec<String>,
    authorities: Vec<String>,
}

#[async_trait]
impl<S> FromRequestParts<S> for Claims
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(req: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let TypedHeader(Authorization(bearer_token)) =
            TypedHeader::<Authorization<Bearer>>::from_request_parts(req, state)
                .await
                .map_err(|_| AuthError::MissingToken)?;

        let header = decode_header(bearer_token.token())?;

        let kid = match header.kid {
            Some(k) => k,
            None => return Err(AuthError::InvalidToken),
        };

        let jwks = get_jwks().await?;

        let decoded_token = match jwks.find(&kid) {
            Some(j) => match j.algorithm {
                AlgorithmParameters::RSA(ref rsa) => {
                    let decoding_key = DecodingKey::from_rsa_components(&rsa.n, &rsa.e).unwrap();

                    let validation = Validation::new(
                        Algorithm::from_str(j.common.key_algorithm.unwrap().to_string().as_str())
                            .unwrap(),
                    );

                    decode::<Claims>(bearer_token.token(), &decoding_key, &validation)
                        .map_err(AuthError::from)
                }
                _ => Err(AuthError::InvalidToken),
            },
            None => Err(AuthError::InvalidToken),
        }?;

        Ok(decoded_token.claims)
    }
}

#[derive(Debug)]
pub struct ReadUser {
    pub username: String,
}

impl From<Claims> for ReadUser {
    fn from(claims: Claims) -> Self {
        ReadUser {
            username: claims.sub,
        }
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for ReadUser
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(req: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let claims = Claims::from_request_parts(req, state).await?;
        let scope = String::from("read");

        if claims.scope.contains(&scope) {
            Ok(ReadUser::from(claims))
        } else {
            Err(AuthError::MissingScope(scope))
        }
    }
}

#[derive(Debug)]
pub struct WriteUser {
    pub username: String,
}

impl From<Claims> for WriteUser {
    fn from(claims: Claims) -> Self {
        WriteUser {
            username: claims.sub,
        }
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for WriteUser
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(req: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let claims = Claims::from_request_parts(req, state).await?;
        let scope = String::from("write");

        if claims.scope.contains(&scope) {
            Ok(WriteUser::from(claims))
        } else {
            Err(AuthError::MissingScope(scope))
        }
    }
}
