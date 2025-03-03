use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::middleware::config::{MiddlewareConfig, MiddlewareType};
use super::error::SettingsError;
use super::Result;

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

        let config = serde_json::from_str(&content)
            .map_err(|e| SettingsError::JsonParseError { 
                source: e 
            })?;
            
        Ok(config)
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
} 