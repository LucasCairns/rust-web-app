use axum::{
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
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashSet,
    env,
    ops::Deref,
    str::FromStr,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use utoipa::ToSchema;

#[derive(thiserror::Error, Debug, Serialize, ToSchema)]
pub enum AuthError {
    #[error("Missing token")]
    MissingToken,
    #[error("Invalid token")]
    InvalidToken,
    #[error("Token expired")]
    ExpiredToken,
    #[error("Unable to verify JWT token")]
    Unavailable,
    #[error("Client requires the scope: {0}")]
    MissingScope(String),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, error_message) = match &self {
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
            "error": format!("{:?}", self)
        }));
        (status, body).into_response()
    }
}

impl From<jsonwebtoken::errors::Error> for AuthError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        match err.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::ExpiredToken,
            _ => AuthError::InvalidToken,
        }
    }
}

static JWK_CACHE: Lazy<RwLock<(Option<JwkSet>, Instant)>> =
    Lazy::new(|| RwLock::new((None, Instant::now())));

async fn get_jwks_cached() -> Result<JwkSet, AuthError> {
    let mut cache = JWK_CACHE.write().await;
    if let (Some(jwks), ts) = (&cache.0, cache.1) {
        if ts.elapsed() < Duration::from_secs(300) {
            return Ok(jwks.clone());
        }
    }

    let auth_url = env::var("AUTH_URL").map_err(|_| AuthError::Unavailable)?;
    let fresh = reqwest::get(format!("{auth_url}/.well-known/jwks.json"))
        .await
        .map_err(|_| AuthError::Unavailable)?
        .json::<JwkSet>()
        .await
        .map_err(|_| AuthError::Unavailable)?;

    *cache = (Some(fresh.clone()), Instant::now());
    Ok(fresh)
}

#[derive(Clone, Debug, Deserialize)]
pub struct Claims {
    pub sub: String,
    #[serde(deserialize_with = "deserialize_scopes")]
    pub scope: HashSet<String>,
    // iss: String,
    // exp: usize,
    // pub authorities: Vec<String>,
}

fn deserialize_scopes<'de, D>(deserializer: D) -> Result<HashSet<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    Ok(raw.split_whitespace().map(|s| s.to_string()).collect())
}

impl<S> FromRequestParts<S> for Claims
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(req: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let TypedHeader(Authorization(bearer)) =
            TypedHeader::<Authorization<Bearer>>::from_request_parts(req, state)
                .await
                .map_err(|_| AuthError::MissingToken)?;

        let header = decode_header(bearer.token())?;
        let kid = header.kid.ok_or(AuthError::InvalidToken)?;

        let jwks = get_jwks_cached().await?;
        let jwk = jwks.find(&kid).ok_or(AuthError::InvalidToken)?;

        let alg = jwk
            .common
            .key_algorithm
            .as_ref()
            .and_then(|alg| Algorithm::from_str(&alg.to_string()).ok())
            .ok_or(AuthError::InvalidToken)?;

        let decoding_key = match &jwk.algorithm {
            AlgorithmParameters::RSA(rsa) => DecodingKey::from_rsa_components(&rsa.n, &rsa.e)
                .map_err(|_| AuthError::InvalidToken)?,
            _ => return Err(AuthError::InvalidToken),
        };

        let validation = Validation::new(alg);
        let token_data = decode::<Claims>(bearer.token(), &decoding_key, &validation)?;
        Ok(token_data.claims)
    }
}

pub trait RequiredScope {
    fn required_scope() -> &'static str;
    fn from_claims(claims: Claims) -> Self;
}

pub struct Scoped<T>(pub T);

impl<S, T> FromRequestParts<S> for Scoped<T>
where
    S: Send + Sync,
    T: RequiredScope + Send,
{
    type Rejection = AuthError;

    async fn from_request_parts(req: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let claims = Claims::from_request_parts(req, state).await?;
        let required = T::required_scope();

        if claims.scope.contains(required) {
            Ok(Scoped(T::from_claims(claims)))
        } else {
            Err(AuthError::MissingScope(required.to_owned()))
        }
    }
}

impl<T> Deref for Scoped<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
pub struct ReadUser {
    pub username: String,
}

impl RequiredScope for ReadUser {
    fn required_scope() -> &'static str {
        "read"
    }

    fn from_claims(claims: Claims) -> Self {
        Self {
            username: claims.sub,
        }
    }
}

#[derive(Debug)]
pub struct WriteUser {
    pub username: String,
}

impl RequiredScope for WriteUser {
    fn required_scope() -> &'static str {
        "write"
    }

    fn from_claims(claims: Claims) -> Self {
        Self {
            username: claims.sub,
        }
    }
}
