mod discord;
mod uploader;

pub use discord::DiscordUploader;
pub use uploader::{BackupMetadata, BackupUploader};

use crate::config::UploadConfig;
use crate::error::Result;

pub fn create_uploaders(config: &UploadConfig) -> Vec<Box<dyn BackupUploader>> {
    let mut uploaders: Vec<Box<dyn BackupUploader>> = Vec::new();

    if let Some(discord_config) = &config.discord {
        uploaders.push(Box::new(DiscordUploader::new(discord_config)));
    }

    uploaders
}
