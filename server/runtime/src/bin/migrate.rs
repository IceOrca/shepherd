use std::error::Error;

use sqlx::postgres::PgPoolOptions;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../migrations");

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    shepherd_runtime::load_environment(&[])?;
    let database_url: String = std::env::var("DATABASE_URL")?;
    let pool: sqlx::PgPool = PgPoolOptions::new().max_connections(1).connect(&database_url).await?;
    MIGRATOR.run(&pool).await?;
    pool.close().await;
    println!("Shepherd database migrations are current.");
    Ok(())
}
