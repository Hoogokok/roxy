use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::middleware::config::{MiddlewareConfig, MiddlewareType};
use super::error::SettingsError;
use super::Result;
use super::converter::{labels_to_json, json_to_labels};

/// JSON 설정 파일을 위한 구조체
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonConfig {
    /// 설정 파일 버전
    #[serde(default = "default_version")]
    pub version: String,
    
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
}

/// 라우터 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterConfig {
    /// 라우팅 규칙
    pub rule: String,
    
    /// 연결된 미들웨어 목록
    #[serde(skip_serializing_if = "Option::is_none")]
    pub middlewares: Option<Vec<String>>,
    
    /// 서비스 이름
    pub service: String,
}

/// 서비스 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub loadbalancer: LoadBalancerConfig,
}

/// 로드밸런서 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadBalancerConfig {
    pub server: ServerConfig,
}

/// 서버 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    
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
fn default_version() -> String {
    "1.0".to_string()
}

fn default_port() -> u16 {
    80
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
        }
    }
}

impl JsonConfig {
    /// JSON 파일에서 설정 로드
    pub async fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = tokio::fs::read_to_string(&path).await.map_err(|e| SettingsError::FileError {
            path: path.as_ref().to_string_lossy().to_string(),
            error: e,
        })?;

        let mut config: Self = serde_json::from_str(&content)
            .map_err(|e| SettingsError::JsonParseError { 
                source: e 
            })?;
        
        // 로드 후 키 정규화 수행
        config.normalize_keys();
            
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
        
        // 설정 키 정규화
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
                                    rule: value.clone(),
                                    middlewares: None,
                                    service: "default".to_string(),
                                });
                            } else if let Some(router) = config.routers.get_mut(router_name) {
                                router.rule = value.clone();
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
                                    .map(|s| s.trim().to_string())
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
                                router.service = value.clone();
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
    pub fn validate(&self) -> Result<()> {
        // 1. 버전 검증
        if !["1.0"].contains(&self.version.as_str()) {
            return Err(SettingsError::InvalidConfig(
                format!("지원하지 않는 버전: {}", self.version)
            ));
        }
        
        // 2. 라우터-서비스 참조 검증
        for (router_name, router) in &self.routers {
            if !self.services.contains_key(&router.service) {
                return Err(SettingsError::InvalidConfig(
                    format!("라우터 '{}'가 존재하지 않는 서비스 '{}'를 참조합니다", 
                            router_name, router.service)
                ));
            }
            
            // 3. 라우터-미들웨어 참조 검증
            if let Some(middlewares) = &router.middlewares {
                for middleware in middlewares {
                    if !self.middlewares.contains_key(middleware) {
                        return Err(SettingsError::InvalidConfig(
                            format!("라우터 '{}'가 존재하지 않는 미들웨어 '{}'를 참조합니다", 
                                    router_name, middleware)
                        ));
                    }
                }
            }
        }
        
        Ok(())
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
        assert_eq!(config.version, "1.0");
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
        config.version = "2.0".to_string();
        let result = config.validate();
        assert!(result.is_err());
        if let Err(SettingsError::InvalidConfig(_)) = result {
            // 예상대로 오류 발생
        } else {
            panic!("Expected InvalidConfig error");
        }
    }

    #[test]
    fn test_validate_router_service() {
        let mut config = JsonConfig::default();
        
        // 서비스가 없는 라우터 추가
        config.routers.insert("test-router".to_string(), RouterConfig {
            rule: "Host(`example.com`)".to_string(),
            middlewares: None,
            service: "non-existent-service".to_string(),
        });
        
        let result = config.validate();
        assert!(result.is_err());
        if let Err(SettingsError::InvalidConfig(_)) = result {
            // 예상대로 오류 발생
        } else {
            panic!("Expected InvalidConfig error");
        }
    }

    #[test]
    fn test_validate_router_middleware() {
        let mut config = JsonConfig::default();
        
        // 서비스 추가
        config.services.insert("test-service".to_string(), ServiceConfig {
            loadbalancer: LoadBalancerConfig {
                server: ServerConfig {
                    port: 80,
                    weight: 1,
                }
            }
        });
        
        // 존재하지 않는 미들웨어를 참조하는 라우터 추가
        config.routers.insert("test-router".to_string(), RouterConfig {
            rule: "Host(`example.com`)".to_string(),
            middlewares: Some(vec!["non-existent-middleware".to_string()]),
            service: "test-service".to_string(),
        });
        
        let result = config.validate();
        assert!(result.is_err());
        if let Err(SettingsError::InvalidConfig(_)) = result {
            // 예상대로 오류 발생
        } else {
            panic!("Expected InvalidConfig error");
        }
    }

    #[test]
    fn test_validate_valid_config() {
        let mut config = JsonConfig::default();
        
        // 미들웨어 추가
        config.middlewares.insert("test-middleware".to_string(), MiddlewareConfig {
            middleware_type: MiddlewareType::Headers,
            enabled: true,
            order: 0,
            settings: HashMap::new(),
        });
        
        // 서비스 추가
        config.services.insert("test-service".to_string(), ServiceConfig {
            loadbalancer: LoadBalancerConfig {
                server: ServerConfig {
                    port: 80,
                    weight: 1,
                }
            }
        });
        
        // 라우터 추가
        config.routers.insert("test-router".to_string(), RouterConfig {
            rule: "Host(`example.com`)".to_string(),
            middlewares: Some(vec!["test-middleware".to_string()]),
            service: "test-service".to_string(),
        });
        
        // 유효한 설정이므로 오류가 없어야 함
        assert!(config.validate().is_ok());
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
        settings.insert("allowOrigins".to_string(), "*".to_string());
        
        config.middlewares.insert("cors".to_string(), MiddlewareConfig {
            middleware_type: MiddlewareType::Cors,
            enabled: true,
            order: 0,
            settings,
        });
        
        // 라우터 설정 추가
        config.routers.insert("api".to_string(), RouterConfig {
            rule: "Host(`api.example.com`)".to_string(),
            middlewares: Some(vec!["cors".to_string()]),
            service: "api-service".to_string(),
        });
        
        // Docker 라벨로 변환
        let labels = config.to_docker_labels("rproxy.http.");
        
        // 결과 확인
        assert_eq!(labels.get("rproxy.http.middlewares.cors.type"), Some(&"cors".to_string()));
        assert_eq!(labels.get("rproxy.http.middlewares.cors.enabled"), Some(&"true".to_string()));
        assert_eq!(labels.get("rproxy.http.middlewares.cors.cors.allowOrigins"), Some(&"*".to_string()));
        assert_eq!(labels.get("rproxy.http.routers.api.rule"), Some(&"Host(`api.example.com`)".to_string()));
        assert_eq!(labels.get("rproxy.http.routers.api.middlewares"), Some(&"cors".to_string()));
        assert_eq!(labels.get("rproxy.http.routers.api.service"), Some(&"api-service".to_string()));
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
            assert!(middleware.settings.contains_key("allowOrigins"));
            assert_eq!(middleware.settings.get("allowOrigins"), Some(&"*".to_string()));
        } else {
            panic!("cors middleware not found");
        }
        
        assert!(config.routers.contains_key("api"));
        if let Some(router) = config.routers.get("api") {
            assert_eq!(router.rule, "Host(`api.example.com`)");
            assert_eq!(router.middlewares, Some(vec!["cors".to_string()]));
            assert_eq!(router.service, "api-service");
        } else {
            panic!("api router not found");
        }
    }
} 