use serde_json::Value;
use super::types::{ValidServiceId, ValidMiddlewareId, ValidRouterId, ValidRule, Version};
use super::error::SettingsError;
use super::json::{JsonConfig, RouterConfig, ServiceConfig};
use std::collections::HashMap;

/// Raw JSON에서 검증된 도메인 모델로의 변환을 처리하는 파서
pub struct ConfigParser;

impl ConfigParser {
    /// Raw JSON 문자열을 파싱하여 검증된 설정으로 변환
    pub fn parse(json_str: &str) -> Result<ValidatedConfig, SettingsError> {
        // 1단계: JSON 파싱
        let raw_json: Value = serde_json::from_str(json_str)
            .map_err(|e| SettingsError::JsonParseError { source: e })?;
        
        // 2단계: JsonConfig로 변환
        let config: JsonConfig = serde_json::from_value(raw_json)
            .map_err(|e| SettingsError::JsonParseError { source: e })?;
        
        // 3단계: 검증된 도메인 모델로 변환
        Self::validate_config(config)
    }
    
    /// JsonConfig를 검증된 도메인 모델로 변환
    fn validate_config(config: JsonConfig) -> Result<ValidatedConfig, SettingsError> {
        // 버전 검증
        let version = Version::new(&config.version)
            .ok_or_else(|| SettingsError::ValidationError {
                field: "version".to_string(),
                message: "Invalid version format".to_string(),
            })?;
        
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
        // 라우팅 규칙 검증
        let rule = ValidRule::new(&config.rule)
            .ok_or_else(|| SettingsError::ValidationError {
                field: "router.rule".to_string(),
                message: "Invalid routing rule format".to_string(),
            })?;
        
        // 서비스 ID 검증
        let service = ValidServiceId::new(&config.service)
            .ok_or_else(|| SettingsError::ValidationError {
                field: "router.service".to_string(),
                message: "Invalid service ID format".to_string(),
            })?;
        
        // 미들웨어 ID 검증
        let middlewares = if let Some(middleware_ids) = config.middlewares {
            let mut validated_ids = Vec::new();
            for id in middleware_ids {
                let valid_id = ValidMiddlewareId::new(&id)
                    .ok_or_else(|| SettingsError::ValidationError {
                        field: "router.middlewares".to_string(),
                        message: format!("Invalid middleware ID format: {}", id),
                    })?;
                validated_ids.push(valid_id);
            }
            Some(validated_ids)
        } else {
            None
        };
        
        Ok(ValidatedRouter {
            rule,
            service,
            middlewares,
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
        let json_str = r#"{
            "version": "invalid",
            "middlewares": {}
        }"#;

        let result = ConfigParser::parse(json_str);
        assert!(result.is_err());
        
        match result.unwrap_err() {
            SettingsError::ValidationError { field, .. } => {
                assert_eq!(field, "version");
            }
            err => panic!("예상치 못한 에러: {:?}", err),
        }
    }

    #[test]
    fn test_parse_invalid_service_id() {
        let json_str = r#"{
            "version": "1.0",
            "services": {
                "invalid service": {
                    "loadbalancer": {
                        "servers": []
                    }
                }
            }
        }"#;

        let result = ConfigParser::parse(json_str);
        assert!(result.is_err());
        
        match result.unwrap_err() {
            SettingsError::ValidationError { field, .. } => {
                assert_eq!(field, "services.invalid service.id");
            }
            err => panic!("예상치 못한 에러: {:?}", err),
        }
    }

    #[test]
    fn test_parse_invalid_router_rule() {
        let json_str = r#"{
            "version": "1.0",
            "routers": {
                "test-router": {
                    "rule": "Host(example.com)",
                    "service": "test-service"
                }
            }
        }"#;

        let result = ConfigParser::parse(json_str);
        assert!(result.is_err());
        
        match result.unwrap_err() {
            SettingsError::ValidationError { field, .. } => {
                assert_eq!(field, "router.rule");
            }
            err => panic!("예상치 못한 에러: {:?}", err),
        }
    }

    #[test]
    fn test_parse_invalid_middleware_id() {
        let json_str = r#"{
            "version": "1.0",
            "routers": {
                "test-router": {
                    "rule": "Host(`example.com`)",
                    "middlewares": ["invalid middleware"],
                    "service": "test-service"
                }
            }
        }"#;

        let result = ConfigParser::parse(json_str);
        assert!(result.is_err());
        
        match result.unwrap_err() {
            SettingsError::ValidationError { field, .. } => {
                assert_eq!(field, "router.middlewares");
            }
            err => panic!("예상치 못한 에러: {:?}", err),
        }
    }

    #[test]
    fn test_parse_invalid_url() {
        let json_str = r#"{
            "version": "1.0",
            "services": {
                "test-service": {
                    "loadbalancer": {
                        "servers": [
                            {
                                "url": "invalid-url",
                                "weight": 1
                            }
                        ]
                    }
                }
            }
        }"#;

        let result = ConfigParser::parse(json_str);
        assert!(result.is_err(), "잘못된 URL이 있는 설정이 파싱되지 않아야 합니다.");
        
        if let Err(err) = result {
            match err {
                SettingsError::ValidationError { field, message } => {
                    assert_eq!(field, "service.loadbalancer.server.url");
                    assert!(message.contains("Invalid URL format"), 
                           "에러 메시지가 URL 형식 오류를 명시해야 합니다.");
                }
                _ => panic!("예상치 못한 에러 타입: {:?}", err),
            }
        }
    }
}