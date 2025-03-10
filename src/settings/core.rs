use std::collections::HashMap;
use crate::middleware::config::{MiddlewareConfig, MiddlewareType};
use crate::settings::types::ValidMiddlewareId;
use crate::settings::typestate::{Validated};
use crate::settings::tls::TlsSettings;
use crate::settings::logging::LogSettings;
use crate::settings::docker::DockerSettings;
use crate::settings::server::{ServerSettings, HttpsDisabled, HttpsEnabled};
use crate::settings::error::SettingsError;

/// 결과 타입 별칭
pub type Result<T> = std::result::Result<T, SettingsError>;

/// 애플리케이션 설정
#[derive(Debug, Clone)]
pub struct Settings<HttpsState = HttpsDisabled> {
    // 타입 매개변수로 인한 역직렬화 문제 방지를 위해 server 필드는 직접 처리
    pub server: ServerSettings<Validated, HttpsState>,
    
    // 로깅 설정
    pub logging: LogSettings<Validated>,
    
    // TLS 설정
    pub tls: TlsSettings<Validated>,

    pub docker: DockerSettings<Validated>,
    
    /// 미들웨어 설정
    pub middleware: HashMap<String, MiddlewareConfig>,
    
    /// 라우터-미들웨어 매핑
    pub router_middlewares: HashMap<String, Vec<ValidMiddlewareId>>,
}

/// HTTP만 지원하는 기본 설정
impl Default for Settings<HttpsDisabled> {
    fn default() -> Self {
        Self {
            server: ServerSettings::default(),
            logging: LogSettings::default(),
            tls: TlsSettings::default(),
            docker: DockerSettings::default(),
            middleware: HashMap::new(),
            router_middlewares: HashMap::new(),
        }
    }
}

/// HTTPS를 지원하는 설정
impl Settings<HttpsEnabled> {
    pub fn create() -> Self {
        Self {
            server: ServerSettings::default(),
            logging: LogSettings::default(),
            tls: TlsSettings::default(),
            docker: DockerSettings::default(),
            middleware: HashMap::new(),
            router_middlewares: HashMap::new(),
        }
    }
}

impl<HttpsState> Settings<HttpsState> {
    /// 설정 유효성 검증
    pub async fn validate(&self) -> Result<()> {
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
                        // 레이트 리밋 설정 검증
                        if !middleware.settings.contains_key("ratelimit.average") {
                            return Err(SettingsError::EnvVarMissing {
                                var_name: format!("{}.ratelimit.average", name),
                            });
                        }
                        if !middleware.settings.contains_key("ratelimit.burst") {
                            return Err(SettingsError::EnvVarMissing {
                                var_name: format!("{}.ratelimit.burst", name),
                            });
                        }
                    }
                    _ => {
                        // 다른 미들웨어 타입 처리
                    }
                }
            }
        }
        
        // 라우터-미들웨어 매핑 검증
        for (router, middlewares) in &self.router_middlewares {
            for middleware_id in middlewares {
                if !self.middleware.contains_key(&middleware_id.to_string()) {
                    return Err(SettingsError::InvalidConfig(
                        format!("Router '{}' references non-existent middleware '{}'", router, middleware_id)
                    ));
                }
            }
        }
        
        Ok(())
    }
    
    /// 미들웨어 추가
    pub fn add_middleware(&mut self, name: String, config: MiddlewareConfig) -> Result<()> {
        if self.middleware.contains_key(&name) {
            return Err(SettingsError::DuplicateMiddleware(name));
        }
        
        self.middleware.insert(name, config);
        Ok(())
    }
    
    /// 미들웨어 추가 (덮어쓰기 옵션 포함)
    pub fn add_middleware_with_override(&mut self, name: String, config: MiddlewareConfig, override_existing: bool) -> Result<()> {
        if self.middleware.contains_key(&name) && !override_existing {
            return Err(SettingsError::DuplicateMiddleware(name));
        }
        
        self.middleware.insert(name, config);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_settings_default() {
        let settings = Settings::<HttpsDisabled>::default();
        assert_eq!(settings.middleware.len(), 0);
        assert_eq!(settings.router_middlewares.len(), 0);
    }
    
    #[test]
    fn test_settings_https_enabled() {
        let settings = Settings::<HttpsEnabled>::create();
        assert_eq!(settings.middleware.len(), 0);
        assert_eq!(settings.router_middlewares.len(), 0);
    }
    
    #[tokio::test]
    async fn test_validate_empty_settings() {
        let settings = Settings::<HttpsDisabled>::default();
        let result = settings.validate().await;
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_add_middleware() {
        let mut settings = Settings::<HttpsDisabled>::default();
        let middleware = MiddlewareConfig {
            middleware_type: MiddlewareType::BasicAuth,
            enabled: true,
            order: 0,
            settings: {
                let mut settings = HashMap::new();
                settings.insert("users".to_string(), "user:pass".to_string());
                settings
            },
        };
        
        let result = settings.add_middleware("test".to_string(), middleware.clone());
        assert!(result.is_ok());
        assert_eq!(settings.middleware.len(), 1);
        
        // 중복 추가 시도
        let result = settings.add_middleware("test".to_string(), middleware);
        assert!(result.is_err());
        
        // 덮어쓰기 옵션으로 추가
        let middleware2 = MiddlewareConfig {
            middleware_type: MiddlewareType::Cors,
            enabled: true,
            order: 1,
            settings: {
                let mut settings = HashMap::new();
                settings.insert("cors.allowOrigins".to_string(), "*".to_string());
                settings
            },
        };
        
        let result = settings.add_middleware_with_override("test".to_string(), middleware2, true);
        assert!(result.is_ok());
        assert_eq!(settings.middleware["test"].middleware_type, MiddlewareType::Cors);
    }
} 