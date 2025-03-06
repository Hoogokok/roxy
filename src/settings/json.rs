use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::debug;

use crate::middleware::config::{MiddlewareConfig, MiddlewareType};
use super::error::SettingsError;
use super::types::{ValidMiddlewareId, ValidRule, ValidServiceId, Version};
use super::{Result, ValidatedConfig};
use super::converter::{labels_to_json, json_to_labels};
use super::validator::JsonConfigValidator;

/// JSON 설정 파일을 위한 구조체
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonConfig {
    /// 설정 파일 버전
    #[serde(default = "default_version")]
    pub version: Version,
    
    /// 설정 고유 ID (선택적)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    
    /// 미들웨어 설정
    #[serde(default)]
    pub middlewares: HashMap<String, MiddlewareConfig>,
    
    /// 라우터 설정
    #[serde(default)]
    pub routers: HashMap<String, RouterConfig>,
    
    /// 서비스 설정
    #[serde(default)]
    pub services: HashMap<String, ServiceConfig>,
    
    /// 라우터-미들웨어 매핑
    #[serde(default)]
    pub router_middlewares: HashMap<String, Vec<String>>,
    
    /// 헬스체크 설정
    #[serde(default)]
    pub health: Option<HealthConfig>,
    
    /// 마지막 유효성 검사 시간
    #[serde(skip)]
    pub last_validated: Option<SystemTime>,
    
    /// 설정 로드 경로
    #[serde(skip)]
    pub source_path: Option<PathBuf>,
}

/// 라우터 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterConfig {
    /// 라우팅 규칙
    pub rule: ValidRule,
    
    /// 연결된 미들웨어 목록
    #[serde(skip_serializing_if = "Option::is_none")]
    pub middlewares: Option<Vec<ValidMiddlewareId>>,
    
    /// 서비스 이름
    pub service: ValidServiceId,
}

/// 서비스 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub loadbalancer: LoadBalancerConfig,
}

/// 로드밸런서 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadBalancerConfig {
    pub servers: Vec<ServerConfig>,
}

/// 서버 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub url: crate::settings::types::ValidUrl,
    
    #[serde(default = "default_weight")]
    pub weight: u32,
}

/// 헬스체크 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    #[serde(default)]
    pub enabled: bool,
    
    pub http: HttpHealthConfig,
    
    #[serde(default = "default_interval")]
    pub interval: u64,
    
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    
    #[serde(default = "default_max_failures")]
    pub max_failures: u32,
}

/// HTTP 헬스체크 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpHealthConfig {
    #[serde(default = "default_health_path")]
    pub path: String,
}

/// 기본 설정값을 위한 함수들
fn default_version() -> Version {
    Version::new("1.0").unwrap()
}

fn default_weight() -> u32 {
    1
}

fn default_interval() -> u64 {
    30
}

fn default_timeout() -> u64 {
    5
}

fn default_max_failures() -> u32 {
    3
}

fn default_health_path() -> String {
    "/health".to_string()
}

impl Default for JsonConfig {
    fn default() -> Self {
        Self {
            version: default_version(),
            id: None,
            middlewares: HashMap::new(),
            routers: HashMap::new(),
            services: HashMap::new(),
            router_middlewares: HashMap::new(),
            health: None,
            last_validated: None,
            source_path: None,
        }
    }
}

impl JsonConfig {
    /// 파일에서 설정 로드
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file_path = path.as_ref();
        let file_content = std::fs::read_to_string(file_path)
            .map_err(|e| SettingsError::FileError { 
                path: file_path.to_string_lossy().to_string(),
                error: e 
            })?;
        
        // 방법 1: 기존 파싱 방식 (유효성 검사 후처리)
        let mut config: Self = serde_json::from_str(&file_content)
            .map_err(|e| SettingsError::JsonParseError { source: e })?;
        
        // 파일 경로 저장
        config.source_path = Some(file_path.to_path_buf());
        
        // 유효성 검증 수행
        config.validate()?;
        
        Ok(config)
    }
    
    /// 파일에서 설정 로드 (강력한 타입 검증 사용)
    pub fn from_file_strongly_typed<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file_path = path.as_ref();
        let file_content = std::fs::read_to_string(file_path)
            .map_err(|e| SettingsError::FileError { 
                path: file_path.to_string_lossy().to_string(),
                error: e 
            })?;
        
        // 방법 2: 새로운 파싱 방식 (타입 시스템을 활용한 유효성 검사)
        // ConfigParser를 사용하여 강력한 타입 검증을 수행
        let validated_config = crate::settings::parser::ConfigParser::parse(&file_content)?;
        
        // 검증된 설정에서 JsonConfig 생성
        let mut config = Self::from_validated_config(validated_config);
        
        // 파일 경로 저장
        config.source_path = Some(file_path.to_path_buf());
        config.last_validated = Some(std::time::SystemTime::now());
        
        Ok(config)
    }
    
    /// 키 형식 정규화
    /// 
    /// Docker 라벨 스타일과 일관된 형식으로 설정 키를 변환합니다.
    /// 예: `basic_auth` → `basicAuth`, `allow_origins` → `allowOrigins` 등
    pub fn normalize_keys(&mut self) {
        // 미들웨어 이름 정규화
        let middleware_keys: Vec<String> = self.middlewares.keys().cloned().collect();
        for key in middleware_keys {
            if key.contains('_') {
                let new_key = Self::to_camel_case(&key);
                if let Some(value) = self.middlewares.remove(&key) {
                    self.middlewares.insert(new_key, value);
                }
            }
        }
        
        // 미들웨어 설정 키 정규화
        for middleware in self.middlewares.values_mut() {
            let setting_keys: Vec<String> = middleware.settings.keys().cloned().collect();
            for key in setting_keys {
                if key.contains('_') {
                    if let Some(value) = middleware.settings.remove(&key) {
                        let new_key = Self::to_camel_case(&key);
                        middleware.settings.insert(new_key, value);
                    }
                }
            }
        }
        
        // 라우터 이름 정규화
        let router_keys: Vec<String> = self.routers.keys().cloned().collect();
        for key in router_keys {
            if key.contains('_') {
                let new_key = Self::to_camel_case(&key);
                if let Some(value) = self.routers.remove(&key) {
                    self.routers.insert(new_key, value);
                }
            }
        }
        
        // 서비스 이름 정규화
        let service_keys: Vec<String> = self.services.keys().cloned().collect();
        for key in service_keys {
            if key.contains('_') {
                let new_key = Self::to_camel_case(&key);
                if let Some(value) = self.services.remove(&key) {
                    self.services.insert(new_key, value);
                }
            }
        }
        
        // 라우터-미들웨어 맵핑 정규화
        let mapping_keys: Vec<String> = self.router_middlewares.keys().cloned().collect();
        for key in mapping_keys {
            if key.contains('_') {
                let new_key = Self::to_camel_case(&key);
                if let Some(value) = self.router_middlewares.remove(&key) {
                    self.router_middlewares.insert(new_key, value);
                }
            }
        }
    }
    
    /// 언더스코어 형식의 문자열을 camelCase로 변환
    fn to_camel_case(s: &str) -> String {
        let mut result = String::new();
        let mut capitalize_next = false;
        
        for (i, c) in s.chars().enumerate() {
            if c == '_' {
                capitalize_next = true;
            } else if capitalize_next {
                result.push(c.to_ascii_uppercase());
                capitalize_next = false;
            } else if i == 0 {
                result.push(c.to_ascii_lowercase());
            } else {
                result.push(c);
            }
        }
        
        result
    }
    
    /// JSON 설정을 Docker 라벨로 변환
    pub fn to_docker_labels(&self, prefix: &str) -> HashMap<String, String> {
        let json_value = serde_json::to_value(self.clone()).unwrap_or_default();
        json_to_labels(&json_value, prefix)
    }
    
    /// Docker 라벨에서 JSON 설정 생성
    pub fn from_docker_labels(labels: &HashMap<String, String>, prefix: &str) -> Self {
        let json = labels_to_json(labels, prefix);
        match serde_json::from_value(json) {
            Ok(config) => config,
            Err(err) => {
                eprintln!("라벨에서 JSON 설정 변환 실패: {}", err);
                
                // 부분적으로 설정 구성
                let mut config = JsonConfig::default();
                
                // 미들웨어 설정 직접 파싱
                for (key, value) in labels {
                    if !key.starts_with(prefix) {
                        continue;
                    }
                    
                    // 미들웨어 타입 추출
                    if key.contains(".type") && key.contains(".middlewares.") {
                        let parts: Vec<&str> = key.split('.').collect();
                        if parts.len() >= 5 {
                            let middleware_name = parts[3];
                            let middleware_type = match value.as_str() {
                                "cors" => MiddlewareType::Cors,
                                "basic-auth" => MiddlewareType::BasicAuth,
                                "ratelimit" => MiddlewareType::RateLimit,
                                "headers" => MiddlewareType::Headers,
                                _ => MiddlewareType::Headers,
                            };
                            
                            // 미들웨어 생성 또는 업데이트
                            if !config.middlewares.contains_key(middleware_name) {
                                config.middlewares.insert(middleware_name.to_string(), MiddlewareConfig {
                                    middleware_type,
                                    enabled: true,
                                    order: 0,
                                    settings: HashMap::new(),
                                });
                            }
                        }
                    }
                    
                    // 라우터 설정 추출
                    if key.contains(".rule") && key.contains(".routers.") {
                        let parts: Vec<&str> = key.split('.').collect();
                        if parts.len() >= 5 {
                            let router_name = parts[3];
                            
                            // 라우터 생성 또는 업데이트
                            if !config.routers.contains_key(router_name) {
                                config.routers.insert(router_name.to_string(), RouterConfig {
                                    rule: crate::settings::types::ValidRule::new(value.clone()).unwrap(),
                                    middlewares: None,
                                    service: crate::settings::types::ValidServiceId::new("default").unwrap(),
                                });
                            } else if let Some(router) = config.routers.get_mut(router_name) {
                                router.rule = crate::settings::types::ValidRule::new(value.clone()).unwrap();
                            }
                        }
                    }
                    
                    // 미들웨어 설정 추출
                    if key.contains(".middlewares.") && 
                       (key.contains(".cors.") || key.contains(".basicAuth.") || 
                        key.contains(".rateLimit.") || key.contains(".headers.")) {
                        let parts: Vec<&str> = key.split('.').collect();
                        if parts.len() >= 6 {
                            let middleware_name = parts[3];
                            let setting_key = parts[5..].join(".");
                            
                            if let Some(middleware) = config.middlewares.get_mut(middleware_name) {
                                middleware.settings.insert(setting_key, value.clone());
                            }
                        }
                    }
                    
                    // 라우터-미들웨어 맵핑 추출
                    if key.contains(".middlewares") && key.contains(".routers.") {
                        let parts: Vec<&str> = key.split('.').collect();
                        if parts.len() >= 5 {
                            let router_name = parts[3];
                            
                            if let Some(router) = config.routers.get_mut(router_name) {
                                router.middlewares = Some(value.split(',')
                                    .map(|s| crate::settings::types::ValidMiddlewareId::new(s.trim()).unwrap())
                                    .collect());
                            }
                        }
                    }
                    
                    // 서비스 설정 추출
                    if key.contains(".service") && key.contains(".routers.") {
                        let parts: Vec<&str> = key.split('.').collect();
                        if parts.len() >= 5 {
                            let router_name = parts[3];
                            
                            if let Some(router) = config.routers.get_mut(router_name) {
                                router.service = crate::settings::types::ValidServiceId::new(value.clone()).unwrap();
                            }
                        }
                    }
                }
                
                config
            }
        }
    }
    
    /// 설정 ID 계산 (파일명 또는 명시적 ID 사용)
    pub fn get_id(&self, file_path: &Path) -> String {
        // 1. JSON에 명시적 ID가 있으면 사용
        if let Some(id) = self.id.as_ref() {
            return id.clone();
        }
        
        // 2. 파일명에서 ID 추출
        file_path.file_stem()
            .and_then(|stem| stem.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }
    
    /// 설정 유효성 검증
    pub fn validate(&mut self) -> Result<()> {
        // 스키마 검증기 초기화
        let validator = JsonConfigValidator::new()?;
        
        // 설정을 JSON으로 직렬화
        let json_str = match serde_json::to_string(self) {
            Ok(s) => s,
            Err(e) => return Err(SettingsError::JsonParseError { source: e }),
        };
        
        // 스키마 검증 수행
        let _value = match validator.validate(&json_str) {
            Ok(v) => v,
            Err(errors) => {
                let file_name = self.source_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                
                return Err(SettingsError::from(errors).into_with_file(file_name));
            }
        };
        
        // 참조 유효성 검사는 validator 내부에서 수행됨
        
        // 검증 성공 시 타임스탬프 업데이트
        self.last_validated = Some(SystemTime::now());
        debug!("설정 검증 성공");
        
        Ok(())
    }

    /// Docker 라벨을 현재 JsonConfig와 병합
    ///
    /// 이 메서드는 Docker 라벨에서 설정을 추출하여 현재 JsonConfig 인스턴스에 병합합니다.
    /// prefix는 Docker 라벨의 접두사를 지정합니다. (예: "rproxy.")
    pub fn merge_with_labels(&mut self, labels: &HashMap<String, String>, prefix: &str) -> Result<()> {
        // 라벨에서 JSON 설정으로 변환
        let json_from_labels = labels_to_json(labels, prefix);
        
        debug!("Docker 라벨에서 JSON 설정 추출: {:?}", json_from_labels);
        
        // JSON에서 JsonConfig로 변환
        if let Ok(config_from_labels) = serde_json::from_value::<JsonConfig>(json_from_labels) {
            // 미들웨어 병합
            for (name, config) in config_from_labels.middlewares {
                if !self.middlewares.contains_key(&name) {
                    debug!("라벨에서 미들웨어 추가: {}", name);
                    self.middlewares.insert(name, config);
                }
            }
            
            // 라우터 병합
            for (name, config) in config_from_labels.routers {
                if !self.routers.contains_key(&name) {
                    debug!("라벨에서 라우터 추가: {}", name);
                    self.routers.insert(name, config);
                }
            }
            
            // 서비스 병합
            for (name, config) in config_from_labels.services {
                if !self.services.contains_key(&name) {
                    debug!("라벨에서 서비스 추가: {}", name);
                    self.services.insert(name, config);
                }
            }
            
            // 라우터-미들웨어 매핑 병합
            for (name, middlewares) in config_from_labels.router_middlewares {
                if !self.router_middlewares.contains_key(&name) {
                    debug!("라벨에서 라우터-미들웨어 매핑 추가: {}", name);
                    self.router_middlewares.insert(name, middlewares);
                }
            }
            
            // 헬스체크 병합
            if self.health.is_none() && config_from_labels.health.is_some() {
                debug!("라벨에서 헬스체크 설정 추가");
                self.health = config_from_labels.health;
            }
        } else {
            debug!("라벨에서 설정 변환 실패, 개별 항목 처리 시도");
            
            // 라벨에서 개별 항목 처리 (상세 구현은 필요에 따라 추가)
        }
        
        Ok(())
    }

    /// ValidatedConfig에서 JsonConfig 인스턴스 생성
    pub fn from_validated_config(validated: ValidatedConfig) -> Self {
        // 서비스 변환
        let mut services = HashMap::new();
        for (id, service) in validated.services {
            // 서버 구성 변환
            let servers = service.loadbalancer.servers.into_iter()
                .map(|s| ServerConfig {
                    url: s.url,
                    weight: s.weight,
                })
                .collect();
            
            let loadbalancer = LoadBalancerConfig {
                servers,
            };
            
            services.insert(id.into_inner(), ServiceConfig { loadbalancer });
        }
        
        // 미들웨어 변환
        let mut middlewares = HashMap::new();
        for (id, middleware_config) in validated.middlewares {
            middlewares.insert(id.into_inner(), middleware_config);
        }
        
        // 라우터 변환
        let mut routers = HashMap::new();
        let mut router_middlewares = HashMap::new();
        
        for (id, router) in validated.routers {
            let router_id = id.into_inner();
            
            // middlewares를 미리 클론하여 소유권 문제 해결
            let middlewares_clone = router.middlewares.clone();
            
            let router_config = RouterConfig {
                rule: router.rule,
                service: router.service,
                middlewares: middlewares_clone,
            };
            
            routers.insert(router_id.clone(), router_config);
            
            // 미들웨어가 있는 경우 라우터-미들웨어 매핑에 추가
            if let Some(mids) = &router.middlewares {
                if !mids.is_empty() {
                    let middleware_ids: Vec<String> = mids.iter()
                        .map(|m| m.clone().into_inner())
                        .collect();
                    router_middlewares.insert(router_id, middleware_ids);
                }
            }
        }
        
        JsonConfig {
            version: validated.version,
            id: None,
            middlewares,
            routers,
            services,
            router_middlewares,
            health: validated.health,
            last_validated: Some(std::time::SystemTime::now()),
            source_path: None,
        }
    }
}

// Extension trait for SettingsError to set file name
trait SettingsErrorExt {
    fn into_with_file(self, file: String) -> SettingsError;
}

impl SettingsErrorExt for SettingsError {
    fn into_with_file(self, file: String) -> SettingsError {
        match self {
            SettingsError::ValidationErrors { errors, .. } => {
                SettingsError::ValidationErrors { errors, file }
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::middleware::config::MiddlewareType;

    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_json_config_default() {
        let config = JsonConfig::default();
        assert_eq!(config.version.as_str(), "1.0");
        assert!(config.id.is_none());
        assert!(config.middlewares.is_empty());
        assert!(config.routers.is_empty());
        assert!(config.services.is_empty());
        assert!(config.router_middlewares.is_empty());
    }

    #[test]
    fn test_get_id_from_file_path() {
        let config = JsonConfig::default();
        let path = PathBuf::from("/tmp/test-config.json");
        assert_eq!(config.get_id(&path), "test-config");
    }

    #[test]
    fn test_get_id_from_explicit_id() {
        let mut config = JsonConfig::default();
        config.id = Some("explicit-id".to_string());
        let path = PathBuf::from("/tmp/test-config.json");
        assert_eq!(config.get_id(&path), "explicit-id");
    }

    #[test]
    fn test_validate_version() {
        let mut config = JsonConfig::default();
        config.version = crate::settings::types::Version::new("2.0").unwrap(); // 지원되지 않는 버전
        
        // 유효한 서비스 추가 (스키마 검증이 가능하도록)
        config.services.insert("test-service".to_string(), ServiceConfig {
            loadbalancer: LoadBalancerConfig {
                servers: vec![ServerConfig {
                    url: crate::settings::types::ValidUrl::new("http://localhost:80").unwrap(),
                    weight: 1,
                }],
            }
        });
        
        // health 설정 추가 (스키마 검증이 가능하도록)
        config.health = Some(HealthConfig {
            enabled: true,
            interval: 10,
            timeout: 5,
            max_failures: 3,
            http: HttpHealthConfig {
                path: "/health".to_string(),
            },
        });
        
        // 미들웨어 추가 (스키마 검증이 가능하도록)
        let mut settings = HashMap::new();
        settings.insert("cors.allowOrigins".to_string(), "*".to_string());
        
        config.middlewares.insert("test-middleware".to_string(), MiddlewareConfig {
            middleware_type: MiddlewareType::Cors,
            enabled: true,
            order: 0,
            settings,
        });
        
        let result = config.validate();
        
        // 버전 검증 실패 확인
        assert!(result.is_err(), "버전 검증이 실패해야 함");
        println!("오류: {:?}", result);
    }

    #[test]
    fn test_validate_router_service() {
        let mut config = JsonConfig::default();
        
        // 서비스가 없는 라우터 추가
        config.routers.insert("test-router".to_string(), RouterConfig {
            rule: crate::settings::types::ValidRule::new("Host(`example.com`)").unwrap(),
            middlewares: None,
            service: crate::settings::types::ValidServiceId::new("non-existent-service").unwrap(),
        });
        
        // 유효한 서비스 추가 (스키마 검증이 가능하도록)
        config.services.insert("test-service".to_string(), ServiceConfig {
            loadbalancer: LoadBalancerConfig {
                servers: vec![ServerConfig {
                    url: crate::settings::types::ValidUrl::new("http://localhost:80").unwrap(),
                    weight: 1,
                }],
            }
        });
        
        // health 설정 추가 (스키마 검증이 가능하도록)
        config.health = Some(HealthConfig {
            enabled: true,
            interval: 10,
            timeout: 5,
            max_failures: 3,
            http: HttpHealthConfig {
                path: "/health".to_string(),
            },
        });
        
        // 미들웨어 추가 (스키마 검증이 가능하도록)
        let mut settings = HashMap::new();
        settings.insert("cors.allowOrigins".to_string(), "*".to_string());
        
        config.middlewares.insert("test-middleware".to_string(), MiddlewareConfig {
            middleware_type: MiddlewareType::Cors,
            enabled: true,
            order: 0,
            settings,
        });
        
        let result = config.validate();
        
        // 서비스 참조 검증 실패 확인
        assert!(result.is_err(), "서비스 참조 검증이 실패해야 함");
        println!("오류: {:?}", result);
    }

    #[test]
    fn test_validate_router_middleware() {
        let mut config = JsonConfig::default();
        
        // 서비스 추가
        config.services.insert("test-service".to_string(), ServiceConfig {
            loadbalancer: LoadBalancerConfig {
                servers: vec![ServerConfig {
                    url: crate::settings::types::ValidUrl::new("http://localhost:80").unwrap(),
                    weight: 1,
                }],
            }
        });
        
        // 존재하지 않는 미들웨어를 참조하는 라우터 추가
        config.routers.insert("test-router".to_string(), RouterConfig {
            rule: crate::settings::types::ValidRule::new("Host(`example.com`)").unwrap(),
            middlewares: Some(vec![crate::settings::types::ValidMiddlewareId::new("non-existent-middleware").unwrap()]),
            service: crate::settings::types::ValidServiceId::new("test-service").unwrap(),
        });
        
        // health 설정 추가 (스키마 검증이 가능하도록)
        config.health = Some(HealthConfig {
            enabled: true,
            interval: 10,
            timeout: 5,
            max_failures: 3,
            http: HttpHealthConfig {
                path: "/health".to_string(),
            },
        });
        
        // 미들웨어 추가 (스키마 검증이 가능하도록)
        let mut settings = HashMap::new();
        settings.insert("cors.allowOrigins".to_string(), "*".to_string());
        
        config.middlewares.insert("test-middleware".to_string(), MiddlewareConfig {
            middleware_type: MiddlewareType::Cors,
            enabled: true,
            order: 0,
            settings,
        });
        
        let result = config.validate();
        
        // 미들웨어 참조 검증 실패 확인
        assert!(result.is_err(), "미들웨어 참조 검증이 실패해야 함");
        println!("오류: {:?}", result);
    }

    #[test]
    fn test_validate_valid_config() {
        let mut config = JsonConfig::default();
        
        // 미들웨어 추가 - 스키마에 맞게 수정
        let mut settings = HashMap::new();
        settings.insert("allowOrigins".to_string(), "*".to_string());
        
        config.middlewares.insert("test-middleware".to_string(), MiddlewareConfig {
            middleware_type: MiddlewareType::Cors, // 스키마에 정의된 타입 사용
            enabled: true,
            order: 0,
            settings,
        });
        
        // 서비스 추가
        config.services.insert("test-service".to_string(), ServiceConfig {
            loadbalancer: LoadBalancerConfig {
                servers: vec![ServerConfig {
                    url: crate::settings::types::ValidUrl::new("http://localhost:80").unwrap(),
                    weight: 1,
                }],
            }
        });
        
        // 라우터 추가
        config.routers.insert("test-router".to_string(), RouterConfig {
            rule: crate::settings::types::ValidRule::new("Host(`example.com`)").unwrap(),
            middlewares: Some(vec![crate::settings::types::ValidMiddlewareId::new("test-middleware").unwrap()]),
            service: crate::settings::types::ValidServiceId::new("test-service").unwrap(),
        });
        
        // health 설정 추가
        config.health = Some(HealthConfig {
            enabled: true,
            interval: 10,
            timeout: 5,
            max_failures: 3,
            http: HttpHealthConfig {
                path: "/health".to_string(),
            },
        });
        
        // 유효한 설정이므로 오류가 없어야 함
        let result = config.validate();
        assert!(result.is_ok(), "유효성 검사 실패: {:?}", result);
    }

    #[test]
    fn test_normalize_keys() {
        let mut config = JsonConfig::default();
        
        // 언더스코어 형식의 미들웨어 설정
        let mut settings = HashMap::new();
        settings.insert("allow_origins".to_string(), "*".to_string());
        
        config.middlewares.insert("test_cors".to_string(), MiddlewareConfig {
            middleware_type: MiddlewareType::Cors,
            enabled: true,
            order: 0,
            settings,
        });
        
        // 키 정규화 수행
        config.normalize_keys();
        
        // 결과 확인: 미들웨어 이름이 camelCase로 변환되어야 함
        assert!(config.middlewares.contains_key("testCors"));
        
        // 설정 키도 camelCase로 변환되어야 함
        if let Some(middleware) = config.middlewares.get("testCors") {
            assert!(middleware.settings.contains_key("allowOrigins"));
            assert_eq!(middleware.settings.get("allowOrigins"), Some(&"*".to_string()));
        } else {
            panic!("testCors middleware not found");
        }
    }

    #[test]
    fn test_to_docker_labels() {
        let mut config = JsonConfig::default();
        
        // 미들웨어 설정 추가
        let mut settings = HashMap::new();
        settings.insert("cors.allowOrigins".to_string(), "*".to_string());
        
        config.middlewares.insert("cors".to_string(), MiddlewareConfig {
            middleware_type: MiddlewareType::Cors,
            enabled: true,
            order: 0,
            settings,
        });
        
        // 서비스 설정 추가 - 변경됨 (server → servers 배열)
        config.services.insert("api-service".to_string(), ServiceConfig {
            loadbalancer: LoadBalancerConfig {
                servers: vec![
                    ServerConfig {
                        url: crate::settings::types::ValidUrl::new("http://localhost:8080").unwrap(),
                        weight: 1,
                    }
                ]
            }
        });
        
        // 라우터 설정 추가
        config.routers.insert("api".to_string(), RouterConfig {
            rule: crate::settings::types::ValidRule::new("Host(`api.example.com`)").unwrap(),
            middlewares: Some(vec![crate::settings::types::ValidMiddlewareId::new("cors").unwrap()]),
            service: crate::settings::types::ValidServiceId::new("api-service").unwrap(),
        });
        
        // Docker 라벨로 변환
        let labels = config.to_docker_labels("rproxy.http.");
        
        // 디버깅을 위해 라벨 출력
        for (key, value) in &labels {
            println!("라벨: {} = {}", key, value);
        }
        
        // 결과 확인
        assert_eq!(labels.get("rproxy.http.middlewares.cors.type"), Some(&"cors".to_string()));
        assert_eq!(labels.get("rproxy.http.middlewares.cors.enabled"), Some(&"true".to_string()));
        assert_eq!(labels.get("rproxy.http.middlewares.cors.order"), Some(&"0".to_string()));
        
        // 라우터 설정 확인
        assert_eq!(labels.get("rproxy.http.routers.api.rule"), Some(&"Host(`api.example.com`)".to_string()));
        assert_eq!(labels.get("rproxy.http.routers.api.middlewares"), Some(&"cors".to_string()));
        assert_eq!(labels.get("rproxy.http.routers.api.service"), Some(&"api-service".to_string()));
        
        // 서비스 URL 확인 (서버 배열 형식으로 변경)
        assert_eq!(labels.get("rproxy.http.services.api-service.loadbalancer.servers.0.url"), 
                  Some(&"http://localhost:8080".to_string()));
        assert_eq!(labels.get("rproxy.http.services.api-service.loadbalancer.servers.0.weight"), 
                  Some(&"1".to_string()));
    }

    #[test]
    fn test_from_docker_labels() {
        // Docker 라벨 생성
        let mut labels = HashMap::new();
        labels.insert("rproxy.http.middlewares.cors.type".to_string(), "cors".to_string());
        labels.insert("rproxy.http.middlewares.cors.enabled".to_string(), "true".to_string());
        labels.insert("rproxy.http.middlewares.cors.cors.allowOrigins".to_string(), "*".to_string());
        labels.insert("rproxy.http.routers.api.rule".to_string(), "Host(`api.example.com`)".to_string());
        labels.insert("rproxy.http.routers.api.middlewares".to_string(), "cors".to_string());
        labels.insert("rproxy.http.routers.api.service".to_string(), "api-service".to_string());
        
        // Docker 라벨에서 설정 생성
        let config = JsonConfig::from_docker_labels(&labels, "rproxy.http.");
        
        // 결과 확인
        assert!(config.middlewares.contains_key("cors"));
        if let Some(middleware) = config.middlewares.get("cors") {
            assert_eq!(middleware.middleware_type, MiddlewareType::Cors);
            assert_eq!(middleware.enabled, true);
            
            // 키 형식 변환 확인 (camelCase → snake_case)
            assert!(middleware.settings.contains_key("allow_origins"));
            assert_eq!(middleware.settings.get("allow_origins"), Some(&"*".to_string()));
        } else {
            panic!("cors middleware not found");
        }
        
        assert!(config.routers.contains_key("api"));
        if let Some(router) = config.routers.get("api") {
            assert_eq!(router.rule.as_str(), "Host(`api.example.com`)");
            assert!(router.middlewares.as_ref().map_or(false, |mids| 
                mids.len() == 1 && mids[0].as_str() == "cors"));
            assert_eq!(router.service.as_str(), "api-service");
        } else {
            panic!("api router not found");
        }
    }
} 