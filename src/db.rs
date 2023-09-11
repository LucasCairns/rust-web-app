use std::{
    env::{self, VarError},
    time::Duration,
};

use sqlx::{
    migrate::MigrateError,
    postgres::{PgConnectOptions, PgPoolOptions},
    ConnectOptions, PgPool,
};
use tracing::log::LevelFilter;

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
    let connect_options = env::var("DATABASE_URL")?
        .parse::<PgConnectOptions>()?
        .log_statements(LevelFilter::Debug)
        .log_slow_statements(LevelFilter::Warn, Duration::from_millis(100));

    let schema_name = env::var("DATABASE_SCHEMA").unwrap_or_else(|_| "public".to_owned());

    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect_with(connect_options.clone())
        .await?;

    sqlx::query(format!("CREATE SCHEMA IF NOT EXISTS {schema_name}").as_str())
        .execute(&pool)
        .await?;

    sqlx::migrate!("db/migrations").run(&pool).await?;

    Ok(pool)
}
