use serde::Deserialize;
use std::env;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub docker_network: String,
    pub label_prefix: String,
    pub http_port: u16,
    pub https_enabled: bool,
    pub https_port: u16,
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,
}

#[derive(Debug)]
pub enum ConfigError {
    EnvVarMissing {
        var_name: String,
    },
    EnvVarInvalid {
        var_name: String,
        value: String,
        reason: String,
    },
    TomlError { error: String },
    FileError { path: String, error: String },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::EnvVarMissing { var_name } => 
                write!(f, "환경 변수 {} 누락", var_name),
            ConfigError::EnvVarInvalid { var_name, value, reason } => 
                write!(f, "환경 변수 {} 값 {} 유효하지 않음: {}", var_name, value, reason),
            ConfigError::TomlError { error } =>
                write!(f, "TOML 파싱 오류: {}", error),
            ConfigError::FileError { path, error } =>
                write!(f, "파일 {} 읽기 오류: {}", path, error),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    pub fn from_toml_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let contents = fs::read_to_string(path.as_ref()).map_err(|e| ConfigError::FileError {
            path: path.as_ref().display().to_string(),
            error: e.to_string(),
        })?;

        toml::from_str(&contents).map_err(|e| ConfigError::TomlError {
            error: e.to_string(),
        })
    }

    pub fn from_env() -> Result<Self, ConfigError> {
        let http_port = std::env::var("HTTP_PORT")
            .map_err(|_| ConfigError::EnvVarMissing { 
                var_name: "HTTP_PORT".to_string() 
            })?
            .parse::<u16>()
            .map_err(|e| ConfigError::EnvVarInvalid { 
                var_name: "HTTP_PORT".to_string(),
                value: std::env::var("HTTP_PORT").unwrap(),
                reason: e.to_string(),
            })?;

        let https_enabled = env::var("HTTPS_ENABLED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);

        let (https_port, tls_cert_path, tls_key_path) = if https_enabled {
            let https_port = env::var("HTTPS_PORT")
                .unwrap_or_else(|_| "443".to_string())
                .parse::<u16>()
                .map_err(|e| ConfigError::EnvVarInvalid { 
                    var_name: "HTTPS_PORT".to_string(),
                    value: env::var("HTTPS_PORT").unwrap_or_else(|_| "443".to_string()),
                    reason: e.to_string(),
                })?;

            let tls_cert_path = env::var("TLS_CERT_PATH").map_err(|_| ConfigError::EnvVarMissing {
                var_name: "TLS_CERT_PATH".to_string(),
            })?;

            let tls_key_path = env::var("TLS_KEY_PATH").map_err(|_| ConfigError::EnvVarMissing {
                var_name: "TLS_KEY_PATH".to_string(),
            })?;

            (https_port, Some(tls_cert_path), Some(tls_key_path))
        } else {
            (443, None, None)
        };

        Ok(Config {
            docker_network: env::var("PROXY_DOCKER_NETWORK")
                .unwrap_or_else(|_| "reverse-proxy-network".to_string()),
            
            label_prefix: env::var("PROXY_LABEL_PREFIX")
                .unwrap_or_else(|_| "reverse-proxy.".to_string()),
            
            http_port,
            https_enabled,
            https_port,
            tls_cert_path,
            tls_key_path,
        })
    }

    pub fn load() -> Result<Self, ConfigError> {
        let config = if let Ok(config_path) = env::var("PROXY_CONFIG_FILE") {
            Self::from_toml_file(config_path)?
        } else {
            Self::from_env()?
        };

        Ok(config)
    }
} 