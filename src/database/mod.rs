mod driver;
mod mysql;

pub use driver::DatabaseDriver;
pub use mysql::MysqlDriver;

use crate::config::{DatabaseConfig, DatabaseEngine};
use crate::error::Result;
pub fn create_driver(config: &DatabaseConfig) -> Result<Box<dyn DatabaseDriver>> {
    match config.engine {
        DatabaseEngine::MySQL => {
            let driver = MysqlDriver::new(config)?;
            Ok(Box::new(driver))
        }
    }
}
