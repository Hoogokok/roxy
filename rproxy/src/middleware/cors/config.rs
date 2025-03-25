use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::marker::PhantomData;
use crate::middleware::utils::get_value_case_insensitive;
use crate::middleware::typestate::{MiddlewareConfigError, MiddlewareValidator};
use crate::settings::typestate::{Raw, TypeState, Validatable, Validated, ValidationErrorCollector};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig<S: TypeState = Raw> {
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
    
    /// 타입스테이트 마커
    #[serde(skip)]
    _state: PhantomData<S>,
}

fn default_methods() -> Vec<String> {
    vec!["GET", "POST", "PUT", "DELETE", "OPTIONS"]
        .into_iter()
        .map(String::from)
        .collect()
}

impl<S: TypeState> Default for CorsConfig<S> {
    fn default() -> Self {
        Self {
            allow_origins: vec![],
            allow_methods: default_methods(),
            allow_headers: vec![],
            expose_headers: vec![],
            max_age: None,
            allow_credentials: false,
            _state: PhantomData,
        }
    }
}

impl CorsConfig<Raw> {
    /// 새로운 설정 생성
    #[allow(dead_code)]
    pub fn new(
        allow_origins: Vec<String>,
        allow_methods: Vec<String>,
        allow_headers: Vec<String>,
        expose_headers: Vec<String>,
        max_age: Option<u32>,
        allow_credentials: bool
    ) -> Self {
        Self {
            allow_origins,
            allow_methods,
            allow_headers,
            expose_headers,
            max_age,
            allow_credentials,
            _state: PhantomData,
        }
    }

    /// Docker 라벨에서 설정을 파싱합니다. (내부용)
    fn parse_labels(labels: &HashMap<String, String>) -> Result<Self, serde_json::Error> {
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

impl CorsConfig<Validated> {
    /// Docker 라벨에서 설정을 파싱하고 곧바로 검증합니다.
    pub fn from_labels(labels: &HashMap<String, String>) -> Result<Self, MiddlewareConfigError> {
        // 1. 원시 설정 파싱
        let raw_config = CorsConfig::<Raw>::parse_labels(labels)
            .map_err(|e| MiddlewareConfigError::ParseError(e.to_string()))?;
        
        // 2. 파싱된 설정 검증
        raw_config.validate()
    }
}

impl Validatable<CorsConfig<Validated>> for CorsConfig<Raw> {
    type Error = MiddlewareConfigError;

    fn validate(self) -> Result<CorsConfig<Validated>, Self::Error> {
        let mut validator = MiddlewareValidator::new();
        validator.start_collecting();
        
        // 1. Origin URL 검증
        for origin in &self.allow_origins {
            if origin != "*" {  // 와일드카드는 허용
                if let Err(_) = url::Url::parse(origin) {
                    validator.add_error(MiddlewareConfigError::InvalidValue {
                        field: "allow_origins".to_string(),
                        message: format!("유효하지 않은 URL 형식: {}", origin),
                    });
                }
            }
        }
        
        // 2. HTTP 메서드 검증
        let valid_methods = ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS", "TRACE", "CONNECT"];
        for method in &self.allow_methods {
            if !valid_methods.contains(&method.as_str()) {
                validator.add_error(MiddlewareConfigError::InvalidValue {
                    field: "allow_methods".to_string(),
                    message: format!("유효하지 않은 HTTP 메서드: {}", method),
                });
            }
        }
        
        // 3. max_age 유효성 검증 (필요하다면)
        // max_age는 양수이므로 Option<u32>로 이미 양수만 가능하여 별도 검증 불필요
        
        // 검증 결과 처리
        if validator.has_errors() {
            let errors = validator.into_errors();
            Err(errors.into_iter().next().unwrap())
        } else {
            Ok(CorsConfig {
                allow_origins: self.allow_origins,
                allow_methods: self.allow_methods,
                allow_headers: self.allow_headers,
                expose_headers: self.expose_headers,
                max_age: self.max_age,
                allow_credentials: self.allow_credentials,
                _state: PhantomData,
            })
        }
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
        
        // 설정 로드 (이제 Validated 버전을 사용)
        let config = CorsConfig::<Validated>::from_labels(&labels).unwrap();
        
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
        let config = CorsConfig::<Validated>::from_labels(&labels).unwrap();
        
        // 기본값 검증
        assert!(config.allow_origins.is_empty());
        assert_eq!(config.allow_methods, vec!["GET", "POST", "PUT", "DELETE", "OPTIONS"]);
        assert!(config.allow_headers.is_empty());
        assert!(config.expose_headers.is_empty());
        assert_eq!(config.max_age, None);
        assert_eq!(config.allow_credentials, false);
    }
    
    #[test]
    fn test_cors_config_validation_success() {
        // 유효한 설정으로 테스트
        let mut config = CorsConfig::<Raw>::default();
        config.allow_origins = vec!["https://example.com".to_string()];
        
        let validated = config.validate();
        assert!(validated.is_ok());
        
        let validated_config = validated.unwrap();
        assert_eq!(validated_config.allow_origins, vec!["https://example.com"]);
    }
    
    #[test]
    fn test_cors_config_validation_empty_origins() {
        // allow_origins가 비어 있어도 유효함 (모든 요청 거부)
        let config = CorsConfig::<Raw>::default();
        
        let validated = config.validate();
        assert!(validated.is_ok());
    }
    
    #[test]
    fn test_cors_config_validation_invalid_origin() {
        // 잘못된 origin URL 형식
        let mut config = CorsConfig::<Raw>::default();
        config.allow_origins = vec!["invalid-url".to_string()];
        
        let validated = config.validate();
        assert!(validated.is_err());
        
        if let Err(err) = validated {
            match err {
                MiddlewareConfigError::InvalidValue { field, .. } => {
                    assert_eq!(field, "allow_origins");
                },
                _ => panic!("잘못된 오류 타입"),
            }
        }
    }
    
    #[test]
    fn test_cors_config_validation_invalid_method() {
        // 잘못된 HTTP 메서드
        let mut config = CorsConfig::<Raw>::default();
        config.allow_methods = vec!["INVALID-METHOD".to_string()];
        
        let validated = config.validate();
        assert!(validated.is_err());
        
        if let Err(err) = validated {
            match err {
                MiddlewareConfigError::InvalidValue { field, .. } => {
                    assert_eq!(field, "allow_methods");
                },
                _ => panic!("잘못된 오류 타입"),
            }
        }
    }
    
    #[test]
    fn test_cors_config_from_labels_with_validation() {
        // 설정 생성 및 검증 통합 테스트
        let mut labels = HashMap::new();
        labels.insert("cors.allowOrigins".to_string(), "https://example.com".to_string());
        
        // 이제 한 단계로 파싱 및 검증
        let validated_config = CorsConfig::<Validated>::from_labels(&labels).unwrap();
        assert_eq!(validated_config.allow_origins, vec!["https://example.com"]);
    }
} 