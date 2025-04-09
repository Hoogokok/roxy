use std::collections::HashMap;
use std::marker::PhantomData;

use crate::middleware::config::{MiddlewareConfig, MiddlewareType};
use crate::settings::types::ValidMiddlewareId;
use crate::settings::typestate::{TypeState, Validated};
use crate::settings::tls::TlsSettings;
use crate::settings::logging::LogSettings;
use crate::settings::docker::DockerSettings;
use crate::settings::server::{ServerSettings, HttpsDisabled, HttpsEnabled};
use crate::settings::error::SettingsError;

use crate::settings::json::ServiceConfig;

/// 결과 타입 별칭
pub type Result<T> = std::result::Result<T, SettingsError>;

/// 애플리케이션 설정
#[derive(Debug, Clone)]
pub struct Settings<State: TypeState = Validated, HttpsState = HttpsDisabled> {
    // 타입 매개변수로 인한 역직렬화 문제 방지를 위해 server 필드는 직접 처리
    pub server: ServerSettings<State, HttpsState>,
    
    // 로깅 설정
    pub logging: LogSettings<State>,
    
    // TLS 설정
    pub tls: TlsSettings<State>,

    pub docker: DockerSettings<State>,
    
    /// 미들웨어 설정
    pub middleware: HashMap<String, MiddlewareConfig>,
    
    /// 라우터-미들웨어 매핑
    pub router_middlewares: HashMap<String, Vec<ValidMiddlewareId>>,
    
    // 서비스별 설정
    pub services: HashMap<String, ServiceConfig>,

    // 상태 마커
    pub _marker: PhantomData<State>,
}

/// HTTP만 지원하는 기본 설정
impl Default for Settings<Validated, HttpsDisabled> {
    fn default() -> Self {
        Self {
            server: ServerSettings::default(),
            logging: LogSettings::default(),
            tls: TlsSettings::default(),
            docker: DockerSettings::default(),
            middleware: HashMap::new(),
            router_middlewares: HashMap::new(),
            services: HashMap::new(),
            _marker: PhantomData,
        }
    }
}

/// HTTPS를 지원하는 설정
impl Settings<Validated, HttpsEnabled> {
    #[cfg(test)]
    pub fn create() -> Self {
        Self {
            server: ServerSettings::default(),
            logging: LogSettings::default(),
            tls: TlsSettings::default(),
            docker: DockerSettings::default(),
            middleware: HashMap::new(),
            router_middlewares: HashMap::new(),
            services: HashMap::new(),
            _marker: PhantomData,
        }
    }
}

impl<State: TypeState, HttpsState> Settings<State, HttpsState> {
    
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

    /// 미들웨어와 라우터-미들웨어 관계 검증 (정적 메서드)
    pub fn validate_middleware_relations(middleware: &HashMap<String, MiddlewareConfig>, 
                                      router_middlewares: &HashMap<String, Vec<ValidMiddlewareId>>) -> Result<()> {
        // 미들웨어 타입별 필수 설정 확인
        for (name, middleware) in middleware {
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
                }
            }
        }
        
        // 라우터-미들웨어 매핑 검증
        for (router, middlewares) in router_middlewares {
            for middleware_id in middlewares {
                if !middleware.contains_key(&middleware_id.to_string()) {
                    return Err(SettingsError::InvalidConfig(
                        format!("Router '{}' references non-existent middleware '{}'", router, middleware_id)
                    ));
                }
            }
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::settings::{json::{LoadBalancerConfig, ServerConfig}, types::ValidUrl};

    use super::*;
    
    #[test]
    fn test_settings_default() {
        let settings = Settings::<Validated, HttpsDisabled>::default();
        assert_eq!(settings.middleware.len(), 0);
        assert_eq!(settings.router_middlewares.len(), 0);
        assert!(settings.services.is_empty());
    }
    
    #[test]
    fn test_settings_https_enabled() {
        let settings = Settings::<Validated, HttpsEnabled>::create();
        assert_eq!(settings.middleware.len(), 0);
        assert_eq!(settings.router_middlewares.len(), 0);
        assert!(settings.services.is_empty());
    }
    
    #[tokio::test]
    async fn test_validate_empty_settings() {
        let settings = Settings::<Validated, HttpsDisabled>::default();
        let result = Settings::<Validated, HttpsDisabled>::validate_middleware_relations(
            &settings.middleware, 
            &settings.router_middlewares
        );
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_add_middleware() {
        let mut settings = Settings::<Validated, HttpsDisabled>::default();
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

    #[tokio::test]
    async fn test_validate_middleware_relations() {
        // 미들웨어와 라우터-미들웨어 맵 준비
        let mut middleware = HashMap::new();
        let mut router_middlewares = HashMap::new();
        
        // 미들웨어 설정
        let basic_auth = MiddlewareConfig {
            middleware_type: MiddlewareType::BasicAuth,
            enabled: true,
            order: 0,
            settings: {
                let mut s = HashMap::new();
                s.insert("users".to_string(), "user:pass".to_string());
                s
            },
        };
        middleware.insert("auth".to_string(), basic_auth);
        
        // 라우터-미들웨어 매핑 (존재하는 미들웨어)
        router_middlewares.insert(
            "router1".to_string(), 
            vec![ValidMiddlewareId::new("auth").unwrap()]
        );
        
        // 유효한 관계 테스트
        let result = Settings::<Validated, HttpsDisabled>::validate_middleware_relations(&middleware, &router_middlewares);
        assert!(result.is_ok());
        
        // 존재하지 않는 미들웨어 참조
        let mut bad_router_middlewares = HashMap::new();
        bad_router_middlewares.insert(
            "router2".to_string(), 
            vec![ValidMiddlewareId::new("non-existent").unwrap()]
        );
        
        let result = Settings::<Validated, HttpsDisabled>::validate_middleware_relations(&middleware, &bad_router_middlewares);
        assert!(result.is_err());
    }

    #[test]
    fn test_settings_stores_service_config() {
        let mut settings = Settings::<Validated, HttpsDisabled>::default();

        let service_config = ServiceConfig {
            loadbalancer: LoadBalancerConfig {
                servers: vec![ServerConfig {
                    url: ValidUrl::new("http://localhost:8080").unwrap(),
                    weight: 5,
                }],
            },
        };
        let service_name = "my-api-service".to_string();
        
        settings.services.insert(service_name.clone(), service_config.clone());

        assert!(settings.services.contains_key(&service_name));
        let retrieved_config = settings.services.get(&service_name).unwrap();
        assert_eq!(retrieved_config.loadbalancer.servers[0].weight, 5);
    }
} 