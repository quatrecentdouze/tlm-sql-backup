use std::fmt;
use std::io;
#[derive(Debug)]
pub enum BackupError {
    Config(String),
    Database(String),
    Compression(String),
    Upload(String),
    Io(io::Error),
    Serialization(String),
}

impl fmt::Display for BackupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackupError::Config(msg) => write!(f, "Configuration error: {}", msg),
            BackupError::Database(msg) => write!(f, "Database error: {}", msg),
            BackupError::Compression(msg) => write!(f, "Compression error: {}", msg),
            BackupError::Upload(msg) => write!(f, "Upload error: {}", msg),
            BackupError::Io(err) => write!(f, "IO error: {}", err),
            BackupError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
        }
    }
}

impl std::error::Error for BackupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BackupError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for BackupError {
    fn from(err: io::Error) -> Self {
        BackupError::Io(err)
    }
}

impl From<toml::de::Error> for BackupError {
    fn from(err: toml::de::Error) -> Self {
        BackupError::Serialization(err.to_string())
    }
}

impl From<toml::ser::Error> for BackupError {
    fn from(err: toml::ser::Error) -> Self {
        BackupError::Serialization(err.to_string())
    }
}

impl From<mysql_async::Error> for BackupError {
    fn from(err: mysql_async::Error) -> Self {
        BackupError::Database(err.to_string())
    }
}

impl From<reqwest::Error> for BackupError {
    fn from(err: reqwest::Error) -> Self {
        BackupError::Upload(err.to_string())
    }
}

impl From<zip::result::ZipError> for BackupError {
    fn from(err: zip::result::ZipError) -> Self {
        BackupError::Compression(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, BackupError>;
