use serde::Deserialize;
use std::io;

/// New config format (config.toml) — sectioned style.
#[derive(Deserialize)]
struct NewConfig {
    server: Option<ServerConfig>,
    database: Option<DbConfig>,
}

#[derive(Deserialize, Default)]
struct ServerConfig {
    host: Option<String>,
    port: Option<u16>,
    output_dir: Option<String>,
}

#[derive(Deserialize, Default)]
struct DbConfig {
    connection_string: Option<String>,
}

#[derive(Debug)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub connection_string: String,
    pub output_dir: String,
}

/// Try loading `config.toml` from the working directory.
/// Returns a default config if the file doesn't exist.
/// Returns a hard parse error for malformed existing files.
pub fn load() -> Result<AppConfig, io::Error> {
    let path = "config.toml";
    if !std::path::Path::new(path).exists() {
        println!("⚠ No config file found. Using defaults.");
        println!("   Please create config.toml in the same folder as this program.");
        return Ok(AppConfig {
            host: "127.0.0.1".to_string(),
            port: 8080,
            connection_string: String::new(),
            output_dir: "payables_output".to_string(),
        });
    }

    let raw = std::fs::read_to_string(path)?;
    let cfg: NewConfig = toml::from_str(&raw).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "config.toml exists but could not be parsed: {}\n   \
                 Common cause: a backslash in a Windows path or password. In TOML, \\\n   \
                 inside \"...\" is an escape character — write paths as C:\\\\Payables \\\n   \
                 (doubled backslashes) or use single quotes: 'C:\\Payables'.",
                e
            ),
        )
    })?;

    let server = cfg.server.unwrap_or_default();
    let db = cfg.database.unwrap_or_default();

    println!("📄 Loaded config from: {}", path);
    Ok(AppConfig {
        host: server.host.unwrap_or_else(|| "127.0.0.1".to_string()),
        port: server.port.unwrap_or(8080),
        connection_string: db.connection_string.unwrap_or_default(),
        output_dir: server.output_dir.unwrap_or_else(|| "payables_output".to_string()),
    })
}
