use std::{collections::HashMap, env, fs, path::Path};
use serde::Deserialize;
use tracing::{debug, info};
use crate::middleware::config::{MiddlewareConfig, MiddlewareType};

mod server;
pub mod logging;
mod tls;
mod error;
pub mod docker;
mod json;

pub use server::ServerSettings;
pub use logging::LogSettings;
pub use tls::TlsSettings;
pub use docker::DockerSettings;
pub use error::SettingsError;
pub use json::JsonConfig;

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

    /// JSON 설정 파일 로드
    pub async fn load_json_config<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let path_ref = path.as_ref();
        debug!("JSON 설정 파일 로드: {}", path_ref.display());
        
        let config = JsonConfig::from_file(&path).await?;
        config.validate()?;
        
        let config_id = config.get_id(path_ref);
        debug!("설정 ID: {}", config_id);
        
        // 미들웨어 설정 병합
        for (name, middleware_config) in config.middlewares {
            let full_name = if name.contains('.') {
                name
            } else {
                format!("{}.{}", config_id, name)
            };
            
            debug!("미들웨어 추가: {}", full_name);
            self.add_middleware(full_name, middleware_config)?;
        }
        
        // 라우터-미들웨어 매핑 병합
        for (router_name, router_config) in config.routers {
            if let Some(middlewares) = router_config.middlewares {
                let full_name = if router_name.contains('.') {
                    router_name
                } else {
                    format!("{}.{}", config_id, router_name)
                };
                
                debug!(
                    router = %full_name,
                    middlewares = ?middlewares,
                    "라우터-미들웨어 매핑 추가"
                );
                
                self.router_middlewares.insert(full_name, middlewares);
            }
        }
        
        info!("JSON 설정 파일 로드 완료: {}", path_ref.display());
        Ok(())
    }
    
    /// 디렉토리에서 모든 JSON 설정 파일 로드
    pub async fn load_config_directory<P: AsRef<Path>>(&mut self, dir_path: P) -> Result<()> {
        let dir_path = dir_path.as_ref();
        debug!("설정 디렉토리 로드: {}", dir_path.display());
        
        let mut read_dir = tokio::fs::read_dir(dir_path).await.map_err(|e| 
            SettingsError::FileError {
                path: dir_path.to_string_lossy().to_string(),
                error: e,
            }
        )?;
        
        let mut loaded_files = 0;
        
        while let Some(entry) = read_dir.next_entry().await.map_err(|e| 
            SettingsError::FileError {
                path: dir_path.to_string_lossy().to_string(),
                error: e,
            }
        )? {
            let path = entry.path();
            
            if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                self.load_json_config(&path).await?;
                loaded_files += 1;
            }
        }
        
        info!("{} JSON 설정 파일 로드됨", loaded_files);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

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

    #[tokio::test]
    async fn test_load_json_config() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test-config.json");
        
        let json_content = r#"{
            "version": "1.0",
            "middlewares": {
                "test-middleware": {
                    "middleware_type": "headers",
                    "enabled": true,
                    "settings": {
                        "headers.customResponseHeaders.X-Test": "value"
                    }
                }
            },
            "routers": {
                "test-router": {
                    "rule": "Host(`example.com`)",
                    "middlewares": ["test-middleware"],
                    "service": "test-service"
                }
            },
            "services": {
                "test-service": {
                    "loadbalancer": {
                        "server": {
                            "port": 8080,
                            "weight": 2
                        }
                    }
                }
            }
        }"#;
        
        let mut file = File::create(&file_path).unwrap();
        file.write_all(json_content.as_bytes()).unwrap();
        
        let mut settings = Settings {
            server: ServerSettings::default(),
            logging: LogSettings::default(),
            tls: TlsSettings::default(),
            docker: DockerSettings::default(),
            middleware: HashMap::new(),
            router_middlewares: HashMap::new(),
        };
        
        // JSON 설정 로드
        settings.load_json_config(&file_path).await.unwrap();
        
        // 설정이 제대로 로드되었는지 검증
        assert_eq!(settings.middleware.len(), 1);
        assert!(settings.middleware.contains_key("test-config.test-middleware"));
        
        let middleware = &settings.middleware["test-config.test-middleware"];
        assert_eq!(middleware.middleware_type, MiddlewareType::Headers);
        assert!(middleware.enabled);
        assert_eq!(middleware.settings.get("headers.customResponseHeaders.X-Test"), Some(&"value".to_string()));
        
        // 라우터-미들웨어 매핑 검증
        assert_eq!(settings.router_middlewares.len(), 1);
        assert!(settings.router_middlewares.contains_key("test-config.test-router"));
        assert_eq!(settings.router_middlewares["test-config.test-router"], vec!["test-middleware"]);
    }
    
    #[tokio::test]
    async fn test_load_config_directory() {
        let dir = tempdir().unwrap();
        
        // 첫 번째 설정 파일 생성
        let first_file_path = dir.path().join("config1.json");
        let first_json = r#"{
            "version": "1.0",
            "middlewares": {
                "cors": {
                    "middleware_type": "cors",
                    "enabled": true,
                    "settings": {
                        "cors.allowOrigins": "*"
                    }
                }
            }
        }"#;
        let mut file = File::create(&first_file_path).unwrap();
        file.write_all(first_json.as_bytes()).unwrap();
        
        // 두 번째 설정 파일 생성
        let second_file_path = dir.path().join("config2.json");
        let second_json = r#"{
            "version": "1.0",
            "middlewares": {
                "auth": {
                    "middleware_type": "basic-auth",
                    "enabled": true,
                    "settings": {
                        "users": "admin:password"
                    }
                }
            },
            "routers": {
                "api": {
                    "rule": "Host(`api.example.com`)",
                    "service": "api"
                }
            }
        }"#;
        let mut file = File::create(&second_file_path).unwrap();
        file.write_all(second_json.as_bytes()).unwrap();
        
        // 설정이 아닌 파일 생성 (무시되어야 함)
        let non_json_path = dir.path().join("README.md");
        let mut file = File::create(&non_json_path).unwrap();
        file.write_all(b"# Test README").unwrap();
        
        // 디렉토리 로드 테스트
        let mut settings = Settings {
            server: ServerSettings::default(),
            logging: LogSettings::default(),
            tls: TlsSettings::default(),
            docker: DockerSettings::default(),
            middleware: HashMap::new(),
            router_middlewares: HashMap::new(),
        };
        
        settings.load_config_directory(dir.path()).await.unwrap();
        
        // 설정이 제대로 로드되었는지 검증
        assert_eq!(settings.middleware.len(), 2);
        assert!(settings.middleware.contains_key("config1.cors"));
        assert!(settings.middleware.contains_key("config2.auth"));
        
        // 각 설정의 내용 검증
        let cors = &settings.middleware["config1.cors"];
        assert_eq!(cors.middleware_type, MiddlewareType::Cors);
        assert!(cors.enabled);
        assert_eq!(cors.settings.get("cors.allowOrigins"), Some(&"*".to_string()));
        
        let auth = &settings.middleware["config2.auth"];
        assert_eq!(auth.middleware_type, MiddlewareType::BasicAuth);
        assert!(auth.enabled);
        assert_eq!(auth.settings.get("users"), Some(&"admin:password".to_string()));
        
        // 라우터 설정 검증
        assert_eq!(settings.router_middlewares.len(), 0); // 라우터에 미들웨어가 없으므로 0
    }
} 