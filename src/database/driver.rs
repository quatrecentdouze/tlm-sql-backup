use crate::error::Result;
use async_trait::async_trait;
use std::io::Write;
#[async_trait]
pub trait DatabaseDriver: Send + Sync {
    async fn test_connection(&self) -> Result<()>;
    async fn list_databases(&self) -> Result<Vec<String>>;
    async fn dump_database(&self, db_name: &str, writer: Box<dyn Write + Send>) -> Result<()>;
    async fn dump_database_silent(&self, db_name: &str, writer: Box<dyn Write + Send>, silent: bool) -> Result<()>;
    fn engine_name(&self) -> &'static str;
}
