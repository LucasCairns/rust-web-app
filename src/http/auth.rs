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
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::HashSet,
    env,
    ops::Deref,
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tracing::error;

#[derive(thiserror::Error, Debug)]
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
            "error": format!("{}", self)
        }));
        (status, body).into_response()
    }
}

impl From<jsonwebtoken::errors::Error> for AuthError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        match err.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::ExpiredToken,
            _ => {
                error!(error = %err, "JWT decode failed");
                AuthError::InvalidToken
            }
        }
    }
}

// Instant timestamp tracks cache age; initialized to now() but unused on first call
// because the Option check guards against returning stale data before a fetch completes.
static JWK_CACHE: Lazy<RwLock<(Option<Arc<JwkSet>>, Instant)>> =
    Lazy::new(|| RwLock::new((None, Instant::now())));

async fn get_jwks_cached() -> Result<Arc<JwkSet>, AuthError> {
    let cache = JWK_CACHE.read().await;
    if let (Some(jwks), elapsed) = (&cache.0, cache.1.elapsed()) {
        if elapsed < Duration::from_secs(300) {
            return Ok(Arc::clone(jwks));
        }
    }
    drop(cache);

    let auth_url = env::var("AUTH_URL").map_err(|_| AuthError::Unavailable)?;
    // Keycloak JWKS endpoint
    let fresh = reqwest::get(format!("{auth_url}/protocol/openid-connect/certs"))
        .await
        .map_err(|_| AuthError::Unavailable)?
        .json::<JwkSet>()
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to fetch JWKS");
            AuthError::Unavailable
        })?;

    let mut write = JWK_CACHE.write().await;
    *write = (Some(Arc::new(fresh)), Instant::now());
    Ok(Arc::clone(write.0.as_ref().unwrap()))
}

#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
pub struct Claims {
    pub sub: String,
    /// Standard OAuth2 scope claim (space-separated).
    #[serde(
        default,
        deserialize_with = "deserialize_scopes"
    )]
    pub scope: HashSet<String>,
    pub iss: String,
    /// Keycloak resource_access roles.
    #[serde(default)]
    pub resource_access: std::collections::HashMap<String, Vec<String>>,
}

fn deserialize_scopes<'de, D>(deserializer: D) -> Result<HashSet<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    Ok(raw.split_whitespace().map(|s| s.to_string()).collect())
}

impl Claims {
    /// Check if this token has the required scope/role.
    ///
    /// Checks both the standard `scope` claim and Keycloak's
    /// `resource_access.{client_id}.roles`.
    fn has_scope(&self, required: &str) -> bool {
        // Check standard OAuth2 scope claim first
        if self.scope.contains(required) {
            return true;
        }

        // Fall back to Keycloak resource_access roles
        let client_id = env::var("AUDIENCE").ok();
        if let Some(ref client) = client_id {
            if let Some(roles) = self.resource_access.get(client.as_str()) {
                if roles.contains(&required.to_string()) {
                    return true;
                }
            }
        }

        false
    }
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
                .map_err(|e| {
                    error!(error = %e, "Failed to build RSA decoding key");
                    AuthError::InvalidToken
                })?,
            _ => return Err(AuthError::InvalidToken),
        };

        let mut validation = Validation::new(alg);
        if let Ok(issuer) = env::var("ISSUER_URL") {
            validation.set_issuer(&[issuer]);
        }
        if let Ok(audience) = env::var("AUDIENCE") {
            validation.set_audience(&[audience]);
        }
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

        if claims.has_scope(required) {
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

macro_rules! define_user {
    ($name:ident, $scope:literal) => {
        #[derive(Debug)]
        pub struct $name {
            pub username: String,
        }

        impl RequiredScope for $name {
            fn required_scope() -> &'static str {
                $scope
            }

            fn from_claims(claims: Claims) -> Self {
                Self {
                    username: claims.sub,
                }
            }
        }
    };
}

define_user!(ReadUser, "read");
define_user!(WriteUser, "write");

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn claims_has_scope_when_scope_claim_present() {
        let mut claims = Claims {
            sub: "user-123".to_string(),
            scope: ["read", "write"]
                .into_iter()
                .map(String::from)
                .collect(),
            iss: "http://localhost:8081/realms/test".to_string(),
            resource_access: HashMap::new(),
        };

        assert!(claims.has_scope("read"));
        assert!(claims.has_scope("write"));
        assert!(!claims.has_scope("admin"));

        // Test with empty scope
        claims.scope.clear();
        assert!(!claims.has_scope("read"));
    }

    #[test]
    fn claims_has_scope_from_resource_access() {
        let mut resource_access = HashMap::new();
        resource_access.insert("rust-web-app".to_string(), vec!["read".to_string()]);

        let claims = Claims {
            sub: "user-456".to_string(),
            scope: HashSet::new(),
            iss: "http://localhost:8081/realms/test".to_string(),
            resource_access,
        };

        // AUDIENCE must be set for resource_access lookup
        std::env::set_var("AUDIENCE", "rust-web-app");
        assert!(claims.has_scope("read"));
        assert!(!claims.has_scope("write"));
    }

    #[test]
    fn claims_has_scope_prefers_standard_scope() {
        let mut resource_access = HashMap::new();
        resource_access.insert(
            "rust-web-app".to_string(),
            vec!["admin".to_string()],
        );

        let mut scope = HashSet::new();
        scope.insert("read".to_string());

        let claims = Claims {
            sub: "user-789".to_string(),
            scope,
            iss: "http://localhost:8081/realms/test".to_string(),
            resource_access,
        };

        std::env::set_var("AUDIENCE", "rust-web-app");
        assert!(claims.has_scope("read")); // from scope claim
        assert!(claims.has_scope("admin")); // from resource_access
    }

    #[test]
    fn deserialize_scopes_from_space_separated_string() {
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct TestClaims {
            #[serde(deserialize_with = "deserialize_scopes")]
            scope: HashSet<String>,
        }

        let json = r#"{"scope": "read write admin"}"#;
        let claims: TestClaims = serde_json::from_str(json).unwrap();

        assert!(claims.scope.contains("read"));
        assert!(claims.scope.contains("write"));
        assert!(claims.scope.contains("admin"));
    }

    #[test]
    fn deserialize_scopes_from_empty_string() {
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct TestClaims {
            #[serde(deserialize_with = "deserialize_scopes")]
            scope: HashSet<String>,
        }

        let json = r#"{"scope": ""}"#;
        let claims: TestClaims = serde_json::from_str(json).unwrap();
        assert!(claims.scope.is_empty());
    }

    #[test]
    fn deserialize_scopes_missing_field_defaults_to_empty() {
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct TestClaims {
            #[serde(default, deserialize_with = "deserialize_scopes")]
            scope: HashSet<String>,
        }

        let json = r#"{}"#;
        let claims: TestClaims = serde_json::from_str(json).unwrap();
        assert!(claims.scope.is_empty());
    }
}
