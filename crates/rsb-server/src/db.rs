use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use crate::config::ServerConfig;

pub async fn init_db(config: &ServerConfig) -> Result<Pool<Postgres>, sqlx::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await?;

    // Run migrations
    sqlx::migrate!().run(&pool).await?;

    println!("✅ Connected to PostgreSQL and migrations applied");
    Ok(pool)
}
