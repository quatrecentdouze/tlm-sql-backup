# TLM Database Backup CLI

A Rust CLI tool for automated MySQL backups with scheduling, Discord upload, and a real-time web dashboard.

## Features

- **Interactive CLI** streamlined Command and Control
- **MySQL Native Backup** - No external tools required
- **Background Scheduler** - Runs backups automatically on intervals
- **Web Dashboard** - Real-time monitoring with modern dark UI
- **Discord Integration** - Uploads backups to a Forum channel
- **Live Log Viewer** - Real-time scheduler logs in CLI

## Installation

```bash
git clone https://github.com/quatrecentdouze/tlm-sql-backup.git
cd tlm-sql-backup
cargo build --release
cargo run --release
```

## Quick Start

On first run, the setup wizard guides you through:
1. Database connection (MySQL host, port, credentials)
2. Selecting databases to backup
3. Backup schedule (minutes/hours/days)
4. Discord bot setup (optional)
5. Web dashboard credentials

## CLI Menu

```
Run backup now (all jobs)    - Execute all backups immediately
Scheduler [RUNNING/STOPPED]  - Start/Stop/View live logs
Web Dashboard [RUNNING/STOPPED] - Start/Stop web UI
Edit configuration           - Modify settings
Test database connection     - Verify MySQL connectivity
Test Discord upload          - Verify bot permissions
```

### Scheduler Submenu
- **Start scheduler** - Runs in background, doesn't block menu
- **Stop scheduler** - Sends shutdown signal
- **View scheduler logs** - Live updating logs (press 'q' to exit)

### Web Dashboard Submenu
- **Start web dashboard** - Starts on configured port
- **Stop web dashboard** - Stops the server
- **View dashboard info** - Shows URL and status

## Web Dashboard

Access at `http://localhost:8080` (default). Features:
- Real-time scheduler status
- Backup history with success/failure tracking
- Total size and success rate metrics
- Discord integration status
- Auto-refresh every 5 seconds

Protected with Basic Auth (configure username/password in setup).

## Configuration

Stored in `~/.db_backup_cli/config.toml`:

```toml
local_backup_dir = "backups"

[[databases]]
name = "production"
engine = "mysql"
host = "localhost"
port = 3306
username = "root"
password = "password"

[[backup_jobs]]
db_config_name = "production"
databases = ["db1", "db2"]

[backup_jobs.schedule]
type = "Hours"
value = 6

[upload.discord]
bot_token = "your-bot-token"
guild_id = 123456789
forum_channel_name = "database-backups"

[web]
enabled = true
port = 8080
username = "admin"
password = "your-password"
```

## Discord Setup

1. Create a bot at [Discord Developer Portal](https://discord.com/developers/applications)
2. Required permissions: Manage Channels, Send Messages, Attach Files, Create Threads
3. Invite bot to your server
4. Copy bot token and guild ID to config

## Graceful Shutdown

- **Ctrl+C once**: Sends shutdown signal, waits for current backup
- **Ctrl+C twice**: Force exit

## 💡 Troubleshooting

### ❗ Linux: `openssl-sys` build error
**Error:** Missing OpenSSL or `pkg-config` during `cargo build --release`.  
**Fix (Debian/Ubuntu):**
```bash
sudo apt install pkg-config libssl-dev
````

**Fix (Fedora/CentOS):**

```bash
sudo dnf install pkg-config openssl-devel
```




## License

MIT
