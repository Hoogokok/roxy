use serde::{Deserialize, Deserializer};
use std::env;
use tracing::Level;
use super::{server::parse_env_var, SettingsError};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Text,
    Json,
}

impl Default for LogFormat {
    fn default() -> Self {
        LogFormat::Text
    }
}

impl std::str::FromStr for LogFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text" => Ok(LogFormat::Text),
            "json" => Ok(LogFormat::Json),
            _ => Err(format!("Invalid log format: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum LogOutput {
    Stdout,
    File(String),
}

impl Default for LogOutput {
    fn default() -> Self {
        LogOutput::Stdout
    }
}

impl std::str::FromStr for LogOutput {
    type Err = String;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "stdout" => Ok(LogOutput::Stdout),
            path => Ok(LogOutput::File(path.to_string())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogSettings {
    pub format: LogFormat,
    pub level: Level,
    pub output: LogOutput,
}

impl LogSettings {
    pub fn from_env() -> Result<Self, SettingsError> {
        Ok(Self {
            format: parse_env_var("PROXY_LOG_FORMAT", || LogFormat::Text)?,
            level: parse_log_level(env::var("PROXY_LOG_LEVEL").unwrap_or_else(|_| "info".to_string()))?,
            output: parse_log_output()?,
        })
    }
}

impl Default for LogSettings {
    fn default() -> Self {
        Self {
            format: LogFormat::default(),
            level: Level::INFO,
            output: LogOutput::default(),
        }
    }
}

fn parse_log_level(level: String) -> Result<Level, SettingsError> {
    match level.to_lowercase().as_str() {
        "error" => Ok(Level::ERROR),
        "warn" => Ok(Level::WARN),
        "info" => Ok(Level::INFO),
        "debug" => Ok(Level::DEBUG),
        "trace" => Ok(Level::TRACE),
        _ => Err(SettingsError::EnvVarInvalid {
            var_name: "PROXY_LOG_LEVEL".to_string(),
            value: level,
            reason: "유효하지 않은 로그 레벨".to_string(),
        }),
    }
}

fn parse_log_output() -> Result<LogOutput, SettingsError> {
    match env::var("PROXY_LOG_OUTPUT") {
        Ok(output) => match output.to_lowercase().as_str() {
            "stdout" => Ok(LogOutput::Stdout),
            path => Ok(LogOutput::File(path.to_string())),
        },
        Err(env::VarError::NotPresent) => Ok(LogOutput::Stdout),
        Err(e) => Err(SettingsError::EnvVarInvalid {
            var_name: "PROXY_LOG_OUTPUT".to_string(),
            value: "".to_string(),
            reason: e.to_string(),
        }),
    }
}

impl<'de> Deserialize<'de> for LogSettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper {
            #[serde(default)]
            format: LogFormat,
            #[serde(default = "default_log_level_string")]
            level: String,
            #[serde(default)]
            output: LogOutput,
        }

        let helper = Helper::deserialize(deserializer)?;
        let level = match helper.level.to_lowercase().as_str() {
            "error" => Level::ERROR,
            "warn" => Level::WARN,
            "info" => Level::INFO,
            "debug" => Level::DEBUG,
            "trace" => Level::TRACE,
            _ => Level::INFO,
        };

        Ok(LogSettings {
            format: helper.format,
            level,
            output: helper.output,
        })
    }
}

fn default_log_level_string() -> String {
    "info".to_string()
} 