use std::env::{self, VarError};

use sqlx::{migrate::MigrateError, postgres::PgPoolOptions, PgPool};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Configuration(#[from] VarError),
    #[error("{0}")]
    Database(#[from] sqlx::Error),
    #[error("{0}")]
    Migrate(#[from] MigrateError),
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
