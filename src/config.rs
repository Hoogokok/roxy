use std::env;

#[derive(Debug, Clone)]
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
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::EnvVarMissing { var_name } => 
                write!(f, "환경 변수 {} 누락", var_name),
            ConfigError::EnvVarInvalid { var_name, value, reason } => 
                write!(f, "환경 변수 {} 값 {} 유효하지 않음: {}", var_name, value, reason),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
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
} 