use auth::Claims;
use axum::{routing::get, Extension, Router, Server};
use sqlx::PgPool;
use tracing::info;
use std::{env, net::SocketAddr};

mod http;
mod auth;

async fn hello(claims: Claims) -> &'static str {
    info!("{:?}", claims);
    "Hello, world!"
}

pub fn app(database_pool: PgPool) -> Router {
    Router::new()
        .route("/", get(hello))
        .merge(http::openapi::router())
        .merge(http::person::router())
        .merge(http::address::router())
        .layer(Extension(database_pool))
}

pub async fn serve(database_pool: PgPool) {
    let server_port = env::var("SERVER_PORT")
        .ok()
        .and_then(|v: String| -> Option<u16> { v.parse().ok() })
        .unwrap_or(8080);

    let addr = SocketAddr::from(([127, 0, 0, 1], server_port));

    tracing::info!("Server listening on: {}", addr);

    Server::bind(&addr)
        .serve(app(database_pool).into_make_service())
        .await
        .expect("Failed to start server")
}
