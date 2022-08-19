mod db;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let database_pool = db::init().await.unwrap();

    rust_web_app::serve(database_pool).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use rust_web_app::app;
    use tower::ServiceExt;

    #[tokio::test]
    async fn hello_route() {
        dotenvy::dotenv().ok();
        let database_pool = db::init().await.unwrap();
        let app = app(database_pool);

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
        dotenvy::dotenv().ok();
        let database_pool = db::init().await.unwrap();
        let app = app(database_pool);

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
