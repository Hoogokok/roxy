use serde::Deserialize;
use serde::Deserializer;
use tracing::warn;
use std::env;
use std::fs;
use std::path::Path;
use tracing::Level;
use std::collections::HashMap;
use serde_json;
use crate::middleware::MiddlewareConfig;

#[derive(Debug, Clone, Deserialize)]
pub enum LogFormat {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "json")]
    Json,
}

#[derive(Debug, Clone, Deserialize)]
pub enum LogOutput {
    #[serde(rename = "stdout")]
    Stdout,
    #[serde(rename = "file")]
    File(String), // 파일 경로
}

#[derive(Debug, Clone)]
pub struct LogConfig {
    pub format: LogFormat,
    pub level: Level,
    pub output: LogOutput,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            format: default_log_format(),
            level: default_log_level(),
            output: default_log_output(),
        }
    }
}

fn default_log_format() -> LogFormat {
    LogFormat::Text
}

fn default_log_level() -> Level {
    Level::INFO
}

fn default_log_output() -> LogOutput {
    LogOutput::Stdout
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub docker_network: String,
    pub label_prefix: String,
    pub http_port: u16,
    pub https_enabled: bool,
    pub https_port: u16,
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,
    
    // 로깅 설정 추가
    #[serde(default)]
    pub logging: LogConfig,

    /// 미들웨어 설정
    #[serde(default)]
    pub middlewares: HashMap<String, MiddlewareConfig>,
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
            
            // 로깅 설정 추가
            logging: LogConfig::default(),

            /// 미들웨어 설정
            middlewares: HashMap::new(), // 환경 변수에서는 기본값으로 빈 맵 사용
        })
    }

    fn apply_env_overrides(&mut self) {
        // HTTP 포트 환경 변수 적용
        if let Ok(port) = env::var("HTTP_PORT")
            .map_err(|_| ())
            .and_then(|p| p.parse::<u16>().map_err(|_| ()))
        {
            self.http_port = port;
        }

        // Docker 네트워크 환경 변수 적용
        if let Ok(network) = env::var("PROXY_DOCKER_NETWORK") {
            self.docker_network = network;
        }

        // 레이블 접두사 환경 변수 적용
        if let Ok(prefix) = env::var("PROXY_LABEL_PREFIX") {
            self.label_prefix = prefix;
        }

        // HTTPS 활성화 환경 변수 적용
        if let Ok(enabled) = env::var("HTTPS_ENABLED") {
            self.https_enabled = enabled.to_lowercase() == "true";
        }

        // HTTPS가 활성화된 경우에만 관련 설정 적용
        if self.https_enabled {
            if let Ok(port) = env::var("HTTPS_PORT")
                .map_err(|_| ())
                .and_then(|p| p.parse::<u16>().map_err(|_| ()))
            {
                self.https_port = port;
            }

            // TLS 인증서 경로 환경 변수 적용
            if let Ok(cert_path) = env::var("TLS_CERT_PATH") {
                self.tls_cert_path = Some(cert_path);
            }

            // TLS 키 경로 환경 변수 적용
            if let Ok(key_path) = env::var("TLS_KEY_PATH") {
                self.tls_key_path = Some(key_path);
            }
        }

        // 로깅 설정 환경 변수 적용
        if let Ok(format) = env::var("LOG_FORMAT") {
            match format.to_lowercase().as_str() {
                "json" => self.logging.format = LogFormat::Json,
                "text" => self.logging.format = LogFormat::Text,
                _ => warn!("잘못된 로그 포맷 지정: {}", format),
            }
        }

        if let Ok(level) = env::var("LOG_LEVEL") {
            match level.to_lowercase().as_str() {
                "error" => self.logging.level = Level::ERROR,
                "warn" => self.logging.level = Level::WARN,
                "info" => self.logging.level = Level::INFO,
                "debug" => self.logging.level = Level::DEBUG,
                "trace" => self.logging.level = Level::TRACE,
                _ => warn!("잘못된 로그 레벨 지정: {}", level),
            }
        }

        if let Ok(output) = env::var("LOG_OUTPUT") {
            if output.to_lowercase() == "stdout" {
                self.logging.output = LogOutput::Stdout;
            } else {
                self.logging.output = LogOutput::File(output);
            }
        }

        // 미들웨어 환경 변수 처리
        // RPROXY_MIDDLEWARE_<NAME>_<KEY>=<VALUE> 형식 지원
        let middleware_prefix = "RPROXY_MIDDLEWARE_";
        for (key, value) in env::vars() {
            if let Some(rest) = key.strip_prefix(middleware_prefix) {
                let parts: Vec<&str> = rest.split('_').collect();
                if parts.len() >= 2 {
                    let name = parts[0].to_lowercase();
                    let config = self.middlewares.entry(name).or_insert_with(|| MiddlewareConfig {
                        middleware_type: "unknown".to_string(),
                        enabled: true,
                        order: 0,
                        settings: HashMap::new(),
                    });

                    match parts[1] {
                        "TYPE" => config.middleware_type = value,
                        "ENABLED" => config.enabled = value.to_lowercase() == "true",
                        "ORDER" => if let Ok(order) = value.parse() {
                            config.order = order;
                        },
                        _ => {
                            let setting_key = parts[1..].join("_").to_lowercase();
                            config.settings.insert(
                                setting_key,
                                serde_json::Value::String(value),
                            );
                        }
                    }
                }
            }
        }
    }

    pub fn load() -> Result<Self, ConfigError> {
        let mut config = if let Ok(config_path) = env::var("PROXY_CONFIG_FILE") {
            Self::from_toml_file(config_path)?
        } else {
            Self::from_env()?
        };

        // Docker 라벨에서 미들웨어 설정 로드
        if let Ok(labels) = env::var("DOCKER_LABELS") {
            if let Ok(labels_map) = serde_json::from_str::<HashMap<String, String>>(&labels) {
                let middleware_configs = MiddlewareConfig::from_labels(&labels_map);
                for (name, middleware_config) in middleware_configs {
                    config.middlewares.insert(name, middleware_config);
                }
            }
        }

        // 환경 변수로 설정 오버라이드
        if env::var("PROXY_CONFIG_FILE").is_ok() {
            config.apply_env_overrides();
        }

        Ok(config)
    }

    pub fn new_for_test() -> Self {
        Self {
            docker_network: "reverse-proxy-network".to_string(),
            label_prefix: "reverse-proxy.".to_string(),
            http_port: 8080,
            https_enabled: false,
            https_port: 443,
            tls_cert_path: None,
            tls_key_path: None,
            logging: LogConfig::default(),
            middlewares: HashMap::new(),
        }
    }
}

impl<'de> Deserialize<'de> for LogConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper {
            #[serde(default = "default_log_format")]
            format: LogFormat,
            #[serde(default = "default_log_level_string")]
            level: String,
            #[serde(default = "default_log_output")]
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

        Ok(LogConfig {
            format: helper.format,
            level,
            output: helper.output,
        })
    }
}

fn default_log_level_string() -> String {
    "info".to_string()
}

