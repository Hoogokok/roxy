use std::{collections::HashMap, env, fs, path::Path};
use serde::Deserialize;
use crate::middleware::config::{MiddlewareConfig, MiddlewareType};

mod server;
mod logging;
mod tls;
mod error;
mod docker;

pub use server::ServerSettings;
pub use logging::LogSettings;
pub use tls::TlsSettings;
pub use docker::DockerSettings;
pub use error::SettingsError;

pub type Result<T> = std::result::Result<T, SettingsError>;
pub use server::parse_env_var;

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    // 서버 설정
    #[serde(default)]
    pub server: ServerSettings,
    
    // 로깅 설정
    #[serde(default)]
    pub logging: LogSettings,
    
    // TLS 설정
    #[serde(default)]
    pub tls: TlsSettings,

    #[serde(default)]
    pub docker: DockerSettings,
    
    /// 미들웨어 설정
    #[serde(default)]
    pub middleware: HashMap<String, MiddlewareConfig>,
}

impl Settings {
    pub fn load() -> Result<Self> {
        if let Ok(config_path) = env::var("PROXY_CONFIG_FILE") {
            Self::from_toml_file(&config_path)
        } else {
            Self::from_env()
        }
    }

    pub fn from_toml_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(&path).map_err(|e| SettingsError::FileError {
            path: path.as_ref().to_string_lossy().to_string(),
            error: e,
        })?;

        toml::from_str(&content).map_err(|e| SettingsError::ParseError { source: e })
    }

    pub fn from_env() -> Result<Self> {
        Ok(Self {
            server: ServerSettings::from_env()?,
            logging: LogSettings::from_env()?,
            tls: TlsSettings::from_env()?,
            docker: DockerSettings::from_env()?,
            middleware: HashMap::new(), // 미들웨어는 Docker 라벨에서 로드
        })
    }

    /// 설정 유효성 검증
    pub async fn validate(&self) -> Result<()> {
        self.server.validate()?;
        self.tls.validate().await?;
        self.docker.validate()?;

        // 미들웨어 설정 검증
        for (name, middleware) in &self.middleware {
            if middleware.enabled {
                match middleware.middleware_type {
                    MiddlewareType::BasicAuth => {
                        if !middleware.settings.contains_key("users") {
                            return Err(SettingsError::EnvVarMissing {
                                var_name: format!("{}.users", name),
                            });
                        }
                    }
                    MiddlewareType::Headers => {
                        // Headers 설정 검증은 필요한 경우 추가
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_from_toml() {
        let toml_content = r#"
            [server]
            http_port = 8080
            https_enabled = true
            https_port = 443

            [logging]
            format = "json"
            level = "info"
            
            [middleware.auth]
            middleware-type = "basic-auth"
            enabled = true
            order = 1
            
            [middleware.auth.settings]
            users = { "admin" = "password" }
        "#;

        let settings: Settings = toml::from_str(toml_content).unwrap();
        assert_eq!(settings.server.http_port, 8080);
        assert!(settings.server.https_enabled);
        assert_eq!(settings.middleware.len(), 1);
    }
} 