use super::uploader::{BackupMetadata, BackupUploader};
use crate::config::DiscordConfig;
use crate::error::{BackupError, Result};
use async_trait::async_trait;
use reqwest::multipart::{Form, Part};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tracing::{debug, error, info, warn};

const DISCORD_API_BASE: &str = "https://discord.com/api/v10";
const MAX_FILE_SIZE: u64 = 8 * 1024 * 1024;

pub struct DiscordUploader {
    config: DiscordConfig,
    client: Client,
}

#[derive(Debug, Deserialize)]
struct Guild {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct Channel {
    id: String,
    name: String,
    #[serde(rename = "type")]
    channel_type: u8,
}

#[derive(Debug, Serialize)]
struct CreateForumChannel {
    name: String,
    #[serde(rename = "type")]
    channel_type: u8,
}

#[derive(Debug, Serialize)]
struct CreateForumPost {
    name: String,
    message: CreateMessage,
}

#[derive(Debug, Serialize)]
struct CreateMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct CreatedThread {
    id: String,
}

impl DiscordUploader {

    pub fn new(config: &DiscordConfig) -> Self {
        let client = Client::builder()
            .user_agent("TLM-SQL-Backup/1.0")
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config: config.clone(),
            client,
        }
    }

    fn auth_header(&self) -> String {
        format!("Bot {}", self.config.bot_token)
    }

    async fn verify_guild_access(&self) -> Result<()> {
        let url = format!("{}/guilds/{}", DISCORD_API_BASE, self.config.guild_id);
        
        let response = self.client
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(BackupError::Upload(format!(
                "Failed to access guild {}: {} - {}",
                self.config.guild_id, status, text
            )));
        }

        let guild: Guild = response.json().await?;
        info!("Verified access to guild: {} ({})", guild.name, guild.id);
        Ok(())
    }

    async fn get_or_create_forum_channel(&self) -> Result<String> {
        let channels = self.get_guild_channels().await?;
        
        for channel in &channels {
            if channel.name == self.config.forum_channel_name && channel.channel_type == 15 {
                debug!("Found existing forum channel: {}", channel.id);
                return Ok(channel.id.clone());
            }
        }

        info!("Creating forum channel: {}", self.config.forum_channel_name);
        self.create_forum_channel().await
    }

    async fn get_guild_channels(&self) -> Result<Vec<Channel>> {
        let url = format!("{}/guilds/{}/channels", DISCORD_API_BASE, self.config.guild_id);
        
        let response = self.client
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(BackupError::Upload(format!(
                "Failed to get guild channels: {} - {}",
                status, text
            )));
        }

        let channels: Vec<Channel> = response.json().await?;
        Ok(channels)
    }

    async fn create_forum_channel(&self) -> Result<String> {
        let url = format!("{}/guilds/{}/channels", DISCORD_API_BASE, self.config.guild_id);
        
        let body = CreateForumChannel {
            name: self.config.forum_channel_name.clone(),
            channel_type: 15,
        };

        let response = self.client
            .post(&url)
            .header("Authorization", self.auth_header())
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(BackupError::Upload(format!(
                "Failed to create forum channel: {} - {}",
                status, text
            )));
        }

        let channel: Channel = response.json().await?;
        info!("Created forum channel: {} ({})", channel.name, channel.id);
        Ok(channel.id)
    }

    async fn create_forum_post(
        &self,
        channel_id: &str,
        metadata: &BackupMetadata,
        file_path: &Path,
        silent: bool,
    ) -> Result<()> {
        let url = format!("{}/channels/{}/threads", DISCORD_API_BASE, channel_id);
        
        let hash_info = metadata.file_hash.as_deref().unwrap_or("N/A");
        let file_size_mb = metadata.file_size as f64 / 1024.0 / 1024.0;
        let db_list = metadata.databases.join(", ");
        
        let message_content = format!(
            "**Database Backup Completed**\n\n\
             🔌 **Connection:** `{}`\n\
             📁 **Databases ({}):** `{}`\n\
             🕐 **Timestamp:** {}\n\
             📊 **File Size:** {:.2} MB\n\
             ⏱️ **Duration:** {} seconds\n\
             🔐 **SHA256:** `{}`\n\
             ✅ **Status:** Success",
            metadata.connection_name,
            metadata.databases.len(),
            db_list,
            metadata.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
            file_size_mb,
            metadata.duration_secs,
            hash_info
        );

        let topic_name = format!(
            "Backup {} - {}",
            metadata.connection_name,
            metadata.timestamp.format("%Y-%m-%d %H:%M")
        );

        if metadata.file_size > MAX_FILE_SIZE {
            warn!(
                "Backup file size ({:.2} MB) exceeds Discord limit ({:.2} MB). Uploading without attachment.",
                file_size_mb,
                MAX_FILE_SIZE as f64 / 1024.0 / 1024.0
            );
            
            let body = CreateForumPost {
                name: topic_name,
                message: CreateMessage {
                    content: format!(
                        "{}\n\n⚠️ **Note:** File too large for Discord upload. Backup saved locally at: `{}`",
                        message_content,
                        metadata.file_path
                    ),
                },
            };

            let response = self.client
                .post(&url)
                .header("Authorization", self.auth_header())
                .json(&body)
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                return Err(BackupError::Upload(format!(
                    "Failed to create forum post: {} - {}",
                    status, text
                )));
            }

            if !silent {
                info!("Created forum post (without attachment due to size limit)");
            }
            return Ok(());
        }

        let mut file = File::open(file_path).await?;
        let mut file_bytes = Vec::new();
        file.read_to_end(&mut file_bytes).await?;

        let file_name = file_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "backup.zip".to_string());
        let file_part = Part::bytes(file_bytes)
            .file_name(file_name.clone())
            .mime_str("application/zip")?;

        let payload_json = serde_json::json!({
            "name": topic_name,
            "message": {
                "content": message_content,
                "attachments": [{
                    "id": 0,
                    "filename": file_name
                }]
            }
        });

        let form = Form::new()
            .text("payload_json", payload_json.to_string())
            .part("files[0]", file_part);

        let response = self.client
            .post(&url)
            .header("Authorization", self.auth_header())
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(BackupError::Upload(format!(
                "Failed to create forum post with attachment: {} - {}",
                status, text
            )));
        }

        let thread: CreatedThread = response.json().await?;
        if !silent {
            info!("Created forum post with attachment: thread ID {}", thread.id);
        }
        Ok(())
    }
}

#[async_trait]
impl BackupUploader for DiscordUploader {
    async fn upload(&self, metadata: &BackupMetadata, file_path: &Path) -> Result<()> {
        self.upload_silent(metadata, file_path, false).await
    }

    async fn upload_silent(&self, metadata: &BackupMetadata, file_path: &Path, silent: bool) -> Result<()> {
        if !silent {
            info!("Uploading backup to Discord forum");
        }

        let channel_id = self.get_or_create_forum_channel().await?;

        self.create_forum_post(&channel_id, metadata, file_path, silent).await?;

        if !silent {
            info!("Discord upload completed successfully");
        }
        Ok(())
    }

    async fn test_connection(&self) -> Result<()> {
        info!("Testing Discord connection...");
        
        self.verify_guild_access().await?;
        
        let _channel_id = self.get_or_create_forum_channel().await?;
        
        info!("Discord connection test successful");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Discord Forum"
    }
}
