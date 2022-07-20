use std::net::SocketAddr;

use axum::{routing::get, Router, Server};

async fn hello() -> &'static str {
    "Hello, world!"
}

fn app() -> Router {
    Router::new().route("/", get(hello))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let server_port = 8080;
    let addr = SocketAddr::from(([127, 0, 0, 1], server_port));

    tracing::debug!("Server listening on: {}", addr);

    Server::bind(&addr)
        .serve(app().into_make_service())
        .await
        .expect("Failed to start server")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn hello_route() {
        let app = app();

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let response_body = hyper::body::to_bytes(response.into_body()).await.unwrap();

        assert_eq!(&response_body[..], "Hello, world!".as_bytes());
    }

    #[tokio::test]
    async fn not_found() {
        let app = app();

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/some-random-endpoint")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
