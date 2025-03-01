use std::{collections::HashMap, env, fs, path::Path};
use serde::Deserialize;
use tracing::debug;
use crate::middleware::config::{MiddlewareConfig, MiddlewareType};

mod server;
pub mod logging;
mod tls;
mod error;
pub mod docker;

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
    
    /// 라우터-미들웨어 매핑
    #[serde(default)]
    pub router_middlewares: HashMap<String, Vec<String>>,
}

impl Settings {
    pub async fn load() -> Result<Self> {
        // 기본 설정만 로드 (Docker 라벨은 ServerManager에서 처리)
        if let Ok(config_path) = env::var("PROXY_CONFIG_FILE") {
            Self::from_toml_file(&config_path).await
        } else {
            Self::from_env().await
        }
    }

    pub async fn from_toml_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(&path).map_err(|e| SettingsError::FileError {
            path: path.as_ref().to_string_lossy().to_string(),
            error: e,
        })?;

        let settings: Self = toml::from_str(&content)
            .map_err(|e| SettingsError::ParseError { source: e })?;
        
        Ok(settings)
    }

    pub async fn from_env() -> Result<Self> {
        let settings = Self {
            server: ServerSettings::from_env()?,
            logging: LogSettings::from_env()?,
            tls: TlsSettings::from_env()?,
            docker: DockerSettings::from_env()?,
            middleware: HashMap::new(),
            router_middlewares: HashMap::new(),
        };

        // 설정 생성 시점에 바로 검증
        settings.validate().await?;
        Ok(settings)
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
                    MiddlewareType::Cors => {
                        // CORS 설정 검증
                        if !middleware.settings.contains_key("cors.allowOrigins") {
                            return Err(SettingsError::EnvVarMissing {
                                var_name: format!("{}.cors.allowOrigins", name),
                            });
                        }
                    }
                    MiddlewareType::RateLimit => {
                        // Rate Limit 필수 설정 검증
                        if !middleware.settings.contains_key("rateLimit.average") {
                            return Err(SettingsError::EnvVarMissing {
                                var_name: format!("{}.rateLimit.average", name),
                            });
                        }
                        
                        // average 값이 유효한 숫자인지 검증
                        if let Some(average) = middleware.settings.get("rateLimit.average") {
                            if average.parse::<u32>().is_err() {
                                return Err(SettingsError::InvalidConfig(
                                    format!("Invalid average value for rate limit: {}", average)
                                ));
                            }
                        }

                        // burst 값이 있다면 유효한 숫자인지 검증
                        if let Some(burst) = middleware.settings.get("rateLimit.burst") {
                            if burst.parse::<u32>().is_err() {
                                return Err(SettingsError::InvalidConfig(
                                    format!("Invalid burst value for rate limit: {}", burst)
                                ));
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    // Docker 라벨을 통해 설정을 추가
    pub fn merge_docker_labels(&mut self, labels: &HashMap<String, String>) -> Result<()> {
        // 미들웨어 설정 파싱
        let label_middlewares = MiddlewareConfig::from_labels(labels)
            .map_err(|e| SettingsError::InvalidConfig(e))?;
        // 미들웨어 추가
        for (name, config) in label_middlewares {
            self.add_middleware(name, config)?;
        }

        // 라우터-미들웨어 매핑 파싱
        self.router_middlewares = Self::parse_router_middlewares(labels);
        
        Ok(())
    }

    pub fn add_middleware(&mut self, name: String, config: MiddlewareConfig) -> Result<()> {
        if self.middleware.contains_key(&name) {
            return Err(SettingsError::DuplicateMiddleware(name));
        }
        self.middleware.insert(name, config);
        Ok(())
    }

    fn parse_router_middlewares(labels: &HashMap<String, String>) -> HashMap<String, Vec<String>> {
        let mut router_middlewares = HashMap::new();
        
        for (key, value) in labels {
            // rproxy.http.routers.{router}.middlewares=middleware1,middleware2
            if let Some(router_config) = key.strip_prefix("rproxy.http.routers.") {
                if router_config.ends_with(".middlewares") {
                    let router_name = router_config.trim_end_matches(".middlewares");
                    let middlewares: Vec<String> = value.split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                    
                    debug!(
                        router = %router_name,
                        middlewares = ?middlewares,
                        "라우터 미들웨어 매핑 파싱"
                    );
                    
                    router_middlewares.insert(router_name.to_string(), middlewares);
                }
            }
        }
        
        router_middlewares
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
            middleware_type = "basic-auth"
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