use std::env::{self, VarError};

use sqlx::{migrate::MigrateError, postgres::PgPoolOptions, PgPool};

#[derive(Debug)]
pub enum Error {
    Configuration(String),
    Query(String),
    Connection(String),
    Migrate(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Configuration(e) => write!(f, "Failed to configure the database: {e}"),
            Error::Query(e) => write!(f, "Failed to execute query: {e}"),
            Error::Connection(e) => write!(f, "Failed to connect to the database: {e}"),
            Error::Migrate(e) => write!(f, "Failed to apply migrations: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<VarError> for Error {
    fn from(error: VarError) -> Self {
        Error::Configuration(error.to_string())
    }
}

impl From<sqlx::Error> for Error {
    fn from(error: sqlx::Error) -> Self {
        match error {
            sqlx::Error::Configuration(e) => Error::Configuration(e.to_string()),
            sqlx::Error::Database(e) => Error::Query(e.to_string()),
            sqlx::Error::Io(e) => Error::Connection(e.to_string()),
            sqlx::Error::Tls(e) => Error::Connection(e.to_string()),
            sqlx::Error::Protocol(e) => Error::Connection(e),
            sqlx::Error::PoolTimedOut => Error::Connection(String::from("Pool timed out")),
            sqlx::Error::PoolClosed => Error::Connection(String::from("Pool closed")),
            sqlx::Error::WorkerCrashed => Error::Connection(String::from("Worker crashed")),
            query_error => Error::Query(query_error.to_string()),
        }
    }
}

impl From<MigrateError> for Error {
    fn from(error: MigrateError) -> Self {
        Error::Migrate(error.to_string())
    }
}

pub async fn init() -> Result<PgPool, Error> {
    let database_url = env::var("DATABASE_URL")?;
    let schema_name = env::var("DATABASE_SCHEMA").unwrap_or_else(|_| "public".to_owned());

    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&database_url)
        .await?;

    sqlx::query(format!("CREATE SCHEMA IF NOT EXISTS {schema_name}").as_str())
        .execute(&pool)
        .await?;

    sqlx::migrate!("db/migrations").run(&pool).await?;

    Ok(pool)
}
