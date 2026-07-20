use std::{
    env::{self, VarError},
    path::Path,
    time::Duration,
};

use once_cell::sync::Lazy;
use sqlx::{
    migrate::MigrateError,
    pool::PoolOptions,
    postgres::{PgConnectOptions, PgPool},
    ConnectOptions,
};
use tracing::info;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Configuration(#[from] VarError),
    #[error("Invalid schema name: {0}")]
    InvalidSchemaName(String),
    #[error("{0}")]
    Database(#[from] sqlx::Error),
    #[error("{0}")]
    Migrate(#[from] MigrateError),
}

static SCHEMA_RE: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r"^([A-Za-z_][0-9A-Za-z_]*)$").unwrap()
});

/// Ensure `name` matches SQL identifier rules.
fn validate_schema_name(name: &str) -> Result<(), Error> {
    const MAX_LEN: usize = 63; // PostgreSQL limit on identifiers
    if name.len() > MAX_LEN {
        return Err(Error::InvalidSchemaName("Exceeds max length".into()));
    }

    if !SCHEMA_RE.is_match(name) {
        return Err(Error::InvalidSchemaName(
            "Must start with letter/underscore, contain no spaces/special chars".into(),
        ));
    }

    Ok(())
}

/// Initialise the database pool, create schema, and run migrations.
pub async fn init() -> Result<PgPool, Error> {
    let database_url = env::var("DATABASE_URL")?;
    let schema_name = env::var("DATABASE_SCHEMA").unwrap_or_else(|_| "public".to_owned());

    validate_schema_name(&schema_name)?;

    let max_connections: u32 = env::var("DB_POOL_SIZE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);

    let options = database_url.parse::<PgConnectOptions>()?
        .options([("search_path", &schema_name)])
        .log_statements(tracing::log::LevelFilter::Debug)
        .log_slow_statements(tracing::log::LevelFilter::Warn, Duration::from_millis(250));

    let pool = PoolOptions::new()
        .max_connections(max_connections)
        .min_connections(2)
        .max_lifetime(Some(Duration::from_secs(30 * 60)))
        .idle_timeout(Some(Duration::from_secs(10 * 60)))
        .connect_with(options)
        .await?;

    sqlx::query(&format!("CREATE SCHEMA IF NOT EXISTS {schema_name}"))
        .execute(&pool)
        .await?;

    sqlx::migrate::Migrator::new(Path::new("db/migrations"))
        .await
        .unwrap()
        .run(&pool)
        .await?;

    info!("Database schema '{}' ready", schema_name);

    Ok(pool)
}
