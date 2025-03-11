use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::middleware::utils::get_value_case_insensitive;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CorsConfig {
    /// 허용할 Origin 목록
    #[serde(default)]
    pub allow_origins: Vec<String>,
    
    /// 허용할 HTTP 메서드 목록
    #[serde(default = "default_methods")]
    pub allow_methods: Vec<String>,
    
    /// 허용할 헤더 목록
    #[serde(default)]
    pub allow_headers: Vec<String>,
    
    /// 노출할 헤더 목록
    #[serde(default)]
    pub expose_headers: Vec<String>,
    
    /// preflight 요청 캐시 시간 (초)
    #[serde(default)]
    pub max_age: Option<u32>,
    
    /// credentials 허용 여부
    #[serde(default)]
    pub allow_credentials: bool,
}

fn default_methods() -> Vec<String> {
    vec!["GET", "POST", "PUT", "DELETE", "OPTIONS"]
        .into_iter()
        .map(String::from)
        .collect()
}

impl CorsConfig {
    pub fn from_labels(labels: &HashMap<String, String>) -> Result<Self, serde_json::Error> {
        let mut config = Self::default();
        
        // 설정값 처리
        // 1. 원본(Origin) 설정
        if let Some(origins) = get_value_case_insensitive(labels, "cors.allowOrigins")
            .or_else(|| get_value_case_insensitive(labels, "cors.allow_origins")) {
            config.allow_origins = origins.split(',')
                .map(|s| s.trim().to_string())
                .collect();
        }
        
        // 2. 메서드 설정
        if let Some(methods) = get_value_case_insensitive(labels, "cors.allowMethods")
            .or_else(|| get_value_case_insensitive(labels, "cors.allow_methods")) {
            config.allow_methods = methods.split(',')
                .map(|s| s.trim().to_string())
                .collect();
        }
        // 메서드는 기본값이 있으므로 빈 값은 무시해야 함
        else if config.allow_methods.is_empty() {
            config.allow_methods = default_methods();
        }
        
        // 3. 허용 헤더 설정
        if let Some(headers) = get_value_case_insensitive(labels, "cors.allowHeaders")
            .or_else(|| get_value_case_insensitive(labels, "cors.allow_headers")) {
            config.allow_headers = headers.split(',')
                .map(|s| s.trim().to_string())
                .collect();
        }
        
        // 4. 노출 헤더 설정
        if let Some(headers) = get_value_case_insensitive(labels, "cors.exposeHeaders")
            .or_else(|| get_value_case_insensitive(labels, "cors.expose_headers")) {
            config.expose_headers = headers.split(',')
                .map(|s| s.trim().to_string())
                .collect();
        }
        
        // 5. Max Age 설정
        if let Some(age) = get_value_case_insensitive(labels, "cors.maxAge")
            .or_else(|| get_value_case_insensitive(labels, "cors.max_age")) {
            config.max_age = age.parse().ok();
        }
        
        // 6. 인증 정보 허용 설정
        if let Some(allow) = get_value_case_insensitive(labels, "cors.allowCredentials")
            .or_else(|| get_value_case_insensitive(labels, "cors.allow_credentials")) {
            config.allow_credentials = allow.parse().unwrap_or(false);
        }
        
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cors_config_from_labels_case_insensitive() {
        // 다양한 케이싱 스타일의 설정값 준비
        let mut labels = HashMap::new();
        
        // 카멜 케이스
        labels.insert("cors.allowOrigins".to_string(), "http://example.com,http://localhost:3000".to_string());
        
        // 대문자
        labels.insert("CORS.ALLOWMETHODS".to_string(), "GET,POST,OPTIONS".to_string());
        
        // 스네이크 케이스
        labels.insert("cors.allow_headers".to_string(), "Content-Type,Authorization".to_string());
        
        // 혼합 케이스
        labels.insert("Cors.expose_Headers".to_string(), "X-Custom-Header".to_string());
        
        // 숫자 설정
        labels.insert("cors.MaxAge".to_string(), "3600".to_string());
        
        // 불리언 설정
        labels.insert("cors.allowCredentials".to_string(), "true".to_string());
        
        // 설정 로드
        let config = CorsConfig::from_labels(&labels).unwrap();
        
        // 설정 검증
        assert_eq!(config.allow_origins, vec!["http://example.com", "http://localhost:3000"]);
        assert_eq!(config.allow_methods, vec!["GET", "POST", "OPTIONS"]);
        assert_eq!(config.allow_headers, vec!["Content-Type", "Authorization"]);
        assert_eq!(config.expose_headers, vec!["X-Custom-Header"]);
        assert_eq!(config.max_age, Some(3600));
        assert_eq!(config.allow_credentials, true);
    }
    
    #[test]
    fn test_cors_config_default_values() {
        // 빈 설정으로 테스트
        let labels = HashMap::new();
        let config = CorsConfig::from_labels(&labels).unwrap();
        
        // 기본값 검증
        assert!(config.allow_origins.is_empty());
        assert_eq!(config.allow_methods, vec!["GET", "POST", "PUT", "DELETE", "OPTIONS"]);
        assert!(config.allow_headers.is_empty());
        assert!(config.expose_headers.is_empty());
        assert_eq!(config.max_age, None);
        assert_eq!(config.allow_credentials, false);
    }
} 