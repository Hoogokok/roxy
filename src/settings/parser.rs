use serde_json::Value;
use super::types::{ValidServiceId, ValidMiddlewareId, ValidRouterId, ValidRule, Version};
use super::error::SettingsError;
use super::json::{JsonConfig, RouterConfig, ServiceConfig};
use std::collections::HashMap;
use std::convert::TryFrom;

/// Raw JSON에서 검증된 도메인 모델로의 변환을 처리하는 파서
pub struct ConfigParser;

/// Raw JSON configuration before validation
#[derive(Debug)]
pub struct RawJsonConfig(Value);

impl TryFrom<&str> for RawJsonConfig {
    type Error = SettingsError;

    fn try_from(json_str: &str) -> Result<Self, Self::Error> {
        let raw_json: Value = serde_json::from_str(json_str)
            .map_err(|e| SettingsError::JsonParseError { source: e })?;
        Ok(RawJsonConfig(raw_json))
    }
}

/// JSON configuration with basic validation
#[derive(Debug, serde::Deserialize)]
pub struct ValidJsonConfig {
    pub version: Version,
    pub services: HashMap<String, ServiceConfig>,
    pub middlewares: HashMap<String, crate::middleware::config::MiddlewareConfig>,
    pub routers: HashMap<String, RouterConfig>,
    pub health: Option<crate::settings::json::HealthConfig>,
}

impl TryFrom<RawJsonConfig> for ValidJsonConfig {
    type Error = SettingsError;

    fn try_from(raw: RawJsonConfig) -> Result<Self, Self::Error> {
        serde_json::from_value(raw.0)
            .map_err(|e| SettingsError::JsonParseError { source: e })
    }
}

impl ConfigParser {
    /// Raw JSON 문자열을 파싱하여 검증된 설정으로 변환
    pub fn parse(json_str: &str) -> Result<ValidatedConfig, SettingsError> {
        let raw_config = RawJsonConfig::try_from(json_str)?;
        let valid_config = ValidJsonConfig::try_from(raw_config)?;
        ValidatedConfig::try_from(valid_config)
    }
    
    /// JsonConfig를 검증된 도메인 모델로 변환
    fn validate_config(config: JsonConfig) -> Result<ValidatedConfig, SettingsError> {
        // 버전은 이미 Version 타입이므로 추가 검증 불필요
        let version = config.version;
        
        // 서비스 ID 검증
        let mut services = HashMap::new();
        for (id, service_config) in config.services {
            let valid_id = ValidServiceId::new(&id)
                .ok_or_else(|| SettingsError::ValidationError {
                    field: format!("services.{}.id", id),
                    message: "Invalid service ID format".to_string(),
                })?;
            
            services.insert(valid_id, Self::validate_service(service_config)?);
        }
        
        // 미들웨어 ID 검증
        let mut middlewares = HashMap::new();
        for (id, middleware_config) in config.middlewares {
            let valid_id = ValidMiddlewareId::new(&id)
                .ok_or_else(|| SettingsError::ValidationError {
                    field: format!("middlewares.{}.id", id),
                    message: "Invalid middleware ID format".to_string(),
                })?;
            
            middlewares.insert(valid_id, middleware_config);
        }
        
        // 라우터 검증
        let mut routers = HashMap::new();
        for (id, router_config) in config.routers {
            let valid_id = ValidRouterId::new(&id)
                .ok_or_else(|| SettingsError::ValidationError {
                    field: format!("routers.{}.id", id),
                    message: "Invalid router ID format".to_string(),
                })?;
            
            routers.insert(valid_id, Self::validate_router(router_config)?);
        }
        
        Ok(ValidatedConfig {
            version,
            services,
            middlewares,
            routers,
            health: config.health,
        })
    }
    
    /// 라우터 설정 검증
    fn validate_router(config: RouterConfig) -> Result<ValidatedRouter, SettingsError> {
        // 이미 각 필드가 강한 타입이므로 추가 검증 불필요
        Ok(ValidatedRouter {
            rule: config.rule,
            service: config.service,
            middlewares: config.middlewares,
        })
    }
    
    /// 서비스 설정 검증
    fn validate_service(config: ServiceConfig) -> Result<ValidatedService, SettingsError> {
        let mut validated_servers = Vec::new();
        
        for server in &config.loadbalancer.servers {
            // 서버 구성 추가
            validated_servers.push(ValidatedServer {
                url: server.url.clone(),
                weight: server.weight,
            });
        }
        
        Ok(ValidatedService {
            loadbalancer: ValidatedLoadBalancer {
                servers: validated_servers,
            },
        })
    }
}

/// 검증된 설정을 나타내는 도메인 모델
#[derive(Debug)]
pub struct ValidatedConfig {
    pub version: Version,
    pub services: HashMap<ValidServiceId, ValidatedService>,
    pub middlewares: HashMap<ValidMiddlewareId, crate::middleware::config::MiddlewareConfig>,
    pub routers: HashMap<ValidRouterId, ValidatedRouter>,
    pub health: Option<crate::settings::json::HealthConfig>,
}

/// 검증된 라우터 설정
#[derive(Debug)]
pub struct ValidatedRouter {
    pub rule: ValidRule,
    pub service: ValidServiceId,
    pub middlewares: Option<Vec<ValidMiddlewareId>>,
}

/// 검증된 서버 설정
#[derive(Debug)]
pub struct ValidatedServer {
    pub url: super::types::ValidUrl,
    pub weight: u32,
}

/// 검증된 로드밸런서 설정
#[derive(Debug)]
pub struct ValidatedLoadBalancer {
    pub servers: Vec<ValidatedServer>,
}

/// 검증된 서비스 설정
#[derive(Debug)]
pub struct ValidatedService {
    pub loadbalancer: ValidatedLoadBalancer,
}

impl TryFrom<ValidJsonConfig> for ValidatedConfig {
    type Error = SettingsError;

    fn try_from(config: ValidJsonConfig) -> Result<Self, Self::Error> {
        // 서비스 ID 검증
        let mut services = HashMap::new();
        for (id, service_config) in config.services {
            let valid_id = ValidServiceId::new(&id)
                .ok_or_else(|| SettingsError::ValidationError {
                    field: format!("services.{}.id", id),
                    message: "Invalid service ID format".to_string(),
                })?;
            
            services.insert(valid_id, ConfigParser::validate_service(service_config)?);
        }
        
        // 미들웨어 ID 검증
        let mut middlewares = HashMap::new();
        for (id, middleware_config) in config.middlewares {
            let valid_id = ValidMiddlewareId::new(&id)
                .ok_or_else(|| SettingsError::ValidationError {
                    field: format!("middlewares.{}.id", id),
                    message: "Invalid middleware ID format".to_string(),
                })?;
            
            middlewares.insert(valid_id, middleware_config);
        }
        
        // 라우터 검증
        let mut routers = HashMap::new();
        for (id, router_config) in config.routers {
            let valid_id = ValidRouterId::new(&id)
                .ok_or_else(|| SettingsError::ValidationError {
                    field: format!("routers.{}.id", id),
                    message: "Invalid router ID format".to_string(),
                })?;
            
            routers.insert(valid_id, ConfigParser::validate_router(router_config)?);
        }
        
        Ok(ValidatedConfig {
            version: config.version,
            services,
            middlewares,
            routers,
            health: config.health,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::middleware::config::MiddlewareType;

    #[test]
    fn test_parse_valid_config() {
        let json_str = r#"{
            "version": "1.0",
            "middlewares": {
                "test-middleware": {
                    "type": "headers",
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
                        "servers": [
                            {
                                "url": "http://localhost:8080",
                                "weight": 1
                            }
                        ]
                    }
                }
            },
            "health": {
                "enabled": true,
                "http": {
                    "path": "/health"
                }
            }
        }"#;

        let result = ConfigParser::parse(json_str);
        assert!(result.is_ok(), "파싱 실패: {:?}", result.err());

        let config = result.unwrap();
        
        // 버전 검증
        assert_eq!(config.version.as_str(), "1.0");
        
        // 미들웨어 검증
        let middleware_id = ValidMiddlewareId::new("test-middleware").unwrap();
        assert!(config.middlewares.contains_key(&middleware_id));
        let middleware = &config.middlewares[&middleware_id];
        assert_eq!(middleware.middleware_type, MiddlewareType::Headers);
        assert!(middleware.enabled);
        assert_eq!(
            middleware.settings.get("headers.customResponseHeaders.X-Test"),
            Some(&"value".to_string())
        );
        
        // 라우터 검증
        let router_id = ValidRouterId::new("test-router").unwrap();
        assert!(config.routers.contains_key(&router_id));
        let router = &config.routers[&router_id];
        assert_eq!(router.rule.as_str(), "Host(`example.com`)");
        assert_eq!(
            router.middlewares.as_ref().map(|m| m.len()),
            Some(1)
        );
        
        // 서비스 검증
        let service_id = ValidServiceId::new("test-service").unwrap();
        assert!(config.services.contains_key(&service_id));
        let service = &config.services[&service_id];
        assert_eq!(service.loadbalancer.servers.len(), 1);
        assert_eq!(
            service.loadbalancer.servers[0].url.as_str(),
            "http://localhost:8080"
        );
    }

    #[test]
    fn test_parse_invalid_version() {
        let json_str = r#"
        {
            "version": "invalid",
            "services": {
                "api": {
                    "loadbalancer": {
                        "servers": [
                            { "url": "http://api.example.com" }
                        ]
                    }
                }
            },
            "routers": {
                "api-router": {
                    "rule": "Host(`example.com`)",
                    "service": "api"
                }
            }
        }
        "#;

        let result = ConfigParser::parse(json_str);
        match result {
            Err(SettingsError::JsonParseError { source }) => {
                assert!(source.to_string().contains("invalid version"));
            },
            Ok(_) => panic!("유효하지 않은 버전이 허용됨"),
            Err(e) => panic!("예상치 못한 에러: {:?}", e),
        }
    }

    #[test]
    fn test_parse_invalid_service_id() {
        let json_str = r#"
        {
            "version": "1.0.0",
            "services": {
                "invalid service": {
                    "loadbalancer": {
                        "servers": [
                            { "url": "http://api.example.com" }
                        ]
                    }
                }
            },
            "routers": {
                "api-router": {
                    "rule": "Host(`example.com`)",
                    "service": "invalid service"
                }
            }
        }
        "#;

        let result = ConfigParser::parse(json_str);
        match result {
            Err(SettingsError::JsonParseError { source }) => {
                assert!(source.to_string().contains("invalid service"));
            },
            Ok(_) => panic!("유효하지 않은 서비스 ID가 허용됨"),
            Err(e) => panic!("예상치 못한 에러: {:?}", e),
        }
    }

    #[test]
    fn test_parse_invalid_router_rule() {
        let json_str = r#"
        {
            "version": "1.0.0",
            "services": {
                "api": {
                    "loadbalancer": {
                        "servers": [
                            { "url": "http://api.example.com" }
                        ]
                    }
                }
            },
            "routers": {
                "api-router": {
                    "rule": "Host(example.com)",
                    "service": "api"
                }
            }
        }
        "#;

        let result = ConfigParser::parse(json_str);
        match result {
            Err(SettingsError::JsonParseError { source }) => {
                assert!(source.to_string().contains("invalid rule"));
            },
            Ok(_) => panic!("유효하지 않은 라우터 규칙이 허용됨"),
            Err(e) => panic!("예상치 못한 에러: {:?}", e),
        }
    }

    #[test]
    fn test_parse_invalid_middleware_id() {
        let json_str = r#"
        {
            "version": "1.0.0",
            "services": {
                "api": {
                    "loadbalancer": {
                        "servers": [
                            { "url": "http://api.example.com" }
                        ]
                    }
                }
            },
            "routers": {
                "api-router": {
                    "rule": "Host(`example.com`)",
                    "service": "api",
                    "middlewares": ["invalid middleware"]
                }
            }
        }
        "#;

        let result = ConfigParser::parse(json_str);
        match result {
            Err(SettingsError::JsonParseError { source }) => {
                assert!(source.to_string().contains("invalid middleware"));
            },
            Ok(_) => panic!("유효하지 않은 미들웨어 ID가 허용됨"),
            Err(e) => panic!("예상치 못한 에러: {:?}", e),
        }
    }

    #[test]
    fn test_parse_invalid_url() {
        let json_str = r#"
        {
            "version": "1.0.0",
            "services": {
                "api": {
                    "loadbalancer": {
                        "servers": [
                            { "url": "invalid-url" }
                        ]
                    }
                }
            },
            "routers": {
                "api-router": {
                    "rule": "Host(`example.com`)",
                    "service": "api"
                }
            }
        }
        "#;

        let result = ConfigParser::parse(json_str);
        match result {
            Err(SettingsError::JsonParseError { source }) => {
                assert!(source.to_string().contains("invalid URL"), "에러 메시지가 'invalid URL'을 포함하지 않음: {}", source);
            },
            Ok(_) => panic!("유효하지 않은 URL이 허용됨"),
            Err(e) => panic!("예상치 못한 에러 타입: {:?}", e),
        }
    }
}