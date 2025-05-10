pub mod store;
pub mod pg_store;

use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;

pub use store::{LedgerStore, LedgerError, TransferQuery, LedgerStats, BatchTransferResponse};
pub use pg_store::PostgresLedgerStore;

/// Create a new connection pool to PostgreSQL
pub async fn create_pg_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(5))
        .connect(database_url)
        .await
}

/// Create a PostgreSQL ledger store
pub async fn create_pg_ledger_store(database_url: &str) -> Result<PostgresLedgerStore, sqlx::Error> {
    let pool = create_pg_pool(database_url).await?;
    
    // Run migrations
    sqlx::migrate!("./src/ledger/migrations")
        .run(&pool)
        .await?;
    
    Ok(PostgresLedgerStore::new(pool))
} 