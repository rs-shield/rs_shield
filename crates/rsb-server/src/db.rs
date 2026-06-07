use sqlx::{SqlitePool, migrate::Migrator};
use std::path::Path;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub async fn init_db(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let pool = SqlitePool::connect(database_url).await?;
    MIGRATOR.run(&pool).await?;
    Ok(pool)
}
