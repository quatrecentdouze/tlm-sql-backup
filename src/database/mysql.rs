use super::driver::DatabaseDriver;
use crate::config::DatabaseConfig;
use crate::error::{BackupError, Result};
use async_trait::async_trait;
use mysql_async::prelude::*;
use mysql_async::{Conn, Opts, OptsBuilder, Pool, Row};
use std::io::Write;
use tracing::{debug, info};
pub struct MysqlDriver {
    pool: Pool,
    config: DatabaseConfig,
}

impl MysqlDriver {
    pub fn new(config: &DatabaseConfig) -> Result<Self> {
        let opts: Opts = OptsBuilder::default()
            .ip_or_hostname(&config.host)
            .tcp_port(config.port)
            .user(Some(&config.username))
            .pass(Some(&config.password))
            .into();

        let pool = Pool::new(opts);
        
        Ok(Self {
            pool,
            config: config.clone(),
        })
    }
    async fn get_conn(&self) -> Result<Conn> {
        self.pool.get_conn().await.map_err(BackupError::from)
    }
    fn escape_string(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('\'', "\\'")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\0', "\\0")
    }
    async fn get_create_table(&self, conn: &mut Conn, db_name: &str, table: &str) -> Result<String> {
        let query = format!("SHOW CREATE TABLE `{}`.`{}`", db_name, table);
        let row: Option<Row> = conn.query_first(&query).await?;
        
        if let Some(row) = row {
            let create_stmt: String = row.get(1).unwrap_or_default();
            Ok(create_stmt)
        } else {
            Err(BackupError::Database(format!(
                "Could not get CREATE TABLE for {}.{}",
                db_name, table
            )))
        }
    }
    async fn get_tables(&self, conn: &mut Conn, db_name: &str) -> Result<Vec<String>> {
        let query = format!("SHOW TABLES FROM `{}`", db_name);
        let tables: Vec<String> = conn.query(query).await?;
        Ok(tables)
    }
    async fn dump_table_data<W: Write + Send>(
        &self,
        conn: &mut Conn,
        db_name: &str,
        table: &str,
        writer: &mut W,
    ) -> Result<()> {
        let columns_query = format!(
            "SELECT COLUMN_NAME FROM INFORMATION_SCHEMA.COLUMNS WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}' ORDER BY ORDINAL_POSITION",
            db_name, table
        );
        let columns: Vec<String> = conn.query(columns_query).await?;
        
        if columns.is_empty() {
            return Ok(());
        }
        let select_query = format!("SELECT * FROM `{}`.`{}`", db_name, table);
        let rows: Vec<Row> = conn.query(select_query).await?;

        if rows.is_empty() {
            return Ok(());
        }
        let batch_size = 100;
        for chunk in rows.chunks(batch_size) {
            let mut insert = format!(
                "INSERT INTO `{}` ({}) VALUES\n",
                table,
                columns.iter().map(|c| format!("`{}`", c)).collect::<Vec<_>>().join(", ")
            );

            let values: Vec<String> = chunk
                .iter()
                .map(|row| {
                    let vals: Vec<String> = (0..columns.len())
                        .map(|i| {
                            match row.get_opt::<mysql_async::Value, _>(i) {
                                Some(Ok(mysql_async::Value::NULL)) => "NULL".to_string(),
                                Some(Ok(mysql_async::Value::Bytes(bytes))) => {
                                    match String::from_utf8(bytes.clone()) {
                                        Ok(s) => format!("'{}'", Self::escape_string(&s)),
                                        Err(_) => {
                                            format!("X'{}'", hex::encode(&bytes))
                                        }
                                    }
                                }
                                Some(Ok(mysql_async::Value::Int(n))) => n.to_string(),
                                Some(Ok(mysql_async::Value::UInt(n))) => n.to_string(),
                                Some(Ok(mysql_async::Value::Float(n))) => n.to_string(),
                                Some(Ok(mysql_async::Value::Double(n))) => n.to_string(),
                                Some(Ok(mysql_async::Value::Date(y, m, d, h, mi, s, us))) => {
                                    format!("'{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:06}'", y, m, d, h, mi, s, us)
                                }
                                Some(Ok(mysql_async::Value::Time(neg, d, h, m, s, us))) => {
                                    let sign = if neg { "-" } else { "" };
                                    format!("'{}{}:{:02}:{:02}.{:06}'", sign, d * 24 + h as u32, m, s, us)
                                }
                                Some(Err(_)) | None => "NULL".to_string(),
                            }
                        })
                        .collect();
                    format!("({})", vals.join(", "))
                })
                .collect();

            insert.push_str(&values.join(",\n"));
            insert.push_str(";\n\n");

            writer.write_all(insert.as_bytes())?;
        }

        Ok(())
    }
}

#[async_trait]
impl DatabaseDriver for MysqlDriver {
    async fn test_connection(&self) -> Result<()> {
        info!("Testing MySQL connection to {}:{}", self.config.host, self.config.port);
        let mut conn = self.get_conn().await?;
        let _: Option<(i32,)> = conn.query_first("SELECT 1").await?;
        info!("MySQL connection successful");
        Ok(())
    }

    async fn list_databases(&self) -> Result<Vec<String>> {
        debug!("Listing MySQL databases");
        let mut conn = self.get_conn().await?;
        let databases: Vec<String> = conn.query("SHOW DATABASES").await?;
        let filtered: Vec<String> = databases
            .into_iter()
            .filter(|db| !matches!(db.as_str(), "information_schema" | "performance_schema" | "mysql" | "sys"))
            .collect();
        
        debug!("Found {} user databases", filtered.len());
        Ok(filtered)
    }

    async fn dump_database(&self, db_name: &str, writer: Box<dyn Write + Send>) -> Result<()> {
        self.dump_database_silent(db_name, writer, false).await
    }

    async fn dump_database_silent(&self, db_name: &str, mut writer: Box<dyn Write + Send>, silent: bool) -> Result<()> {
        if !silent {
            info!("Starting dump of database: {}", db_name);
        }
        let mut conn = self.get_conn().await?;
        let header = format!(
            "-- MySQL dump generated by tlm-sql-backup\n\
             -- Database: {}\n\
             -- Generated at: {}\n\n\
             SET FOREIGN_KEY_CHECKS=0;\n\
             SET SQL_MODE='NO_AUTO_VALUE_ON_ZERO';\n\n",
            db_name,
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
        );
        writer.write_all(header.as_bytes())?;
        let tables = self.get_tables(&mut conn, db_name).await?;
        if !silent {
            info!("Found {} tables in database {}", tables.len(), db_name);
        }

        for table in &tables {
            if !silent {
                debug!("Dumping table: {}", table);
            }
            let table_header = format!("\n-- Table: {}\n-- ----------------------------------------\n\n", table);
            writer.write_all(table_header.as_bytes())?;
            let drop_stmt = format!("DROP TABLE IF EXISTS `{}`;\n\n", table);
            writer.write_all(drop_stmt.as_bytes())?;
            let create_stmt = self.get_create_table(&mut conn, db_name, table).await?;
            writer.write_all(create_stmt.as_bytes())?;
            writer.write_all(b";\n\n")?;
            self.dump_table_data(&mut conn, db_name, table, &mut writer).await?;
        }
        let footer = "\nSET FOREIGN_KEY_CHECKS=1;\n";
        writer.write_all(footer.as_bytes())?;

        if !silent {
            info!("Completed dump of database: {}", db_name);
        }
        Ok(())
    }

    fn engine_name(&self) -> &'static str {
        "MySQL"
    }
}

impl Drop for MysqlDriver {
    fn drop(&mut self) {
    }
}
