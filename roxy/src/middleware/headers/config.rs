use hyper::header::{HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error};
use std::str::FromStr;
use std::marker::PhantomData;
use crate::middleware::typestate::{MiddlewareConfigError, MiddlewareValidator};
use crate::middleware::MiddlewareError;
use crate::settings::typestate::{Raw, TypeState, Validatable, Validated, ValidationErrorCollector};

/// 헤더 수정 작업 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderModification<S: TypeState = Raw> {
    /// 추가할 헤더
    #[serde(default)]
    pub add: HashMap<String, String>,
    
    /// 삭제할 헤더
    #[serde(default)]
    pub remove: Vec<String>,
    
    /// 덮어쓸 헤더
    #[serde(default)]
    pub set: HashMap<String, String>,
    
    /// 타입스테이트 마커
    #[serde(skip)]
    _state: PhantomData<S>,
}

impl<S: TypeState> Default for HeaderModification<S> {
    fn default() -> Self {
        Self {
            add: HashMap::new(),
            remove: Vec::new(),
            set: HashMap::new(),
            _state: PhantomData,
        }
    }
}

impl<S: TypeState> HeaderModification<S> {
    /// 헤더 맵에 설정된 수정사항을 적용합니다.
    pub fn apply_to_headers(&self, headers: &mut hyper::HeaderMap) {
        debug!("헤더 수정 시작: add={:?}, remove={:?}, set={:?}", self.add, self.remove, self.set);
        
        // 1. 먼저 삭제할 헤더 처리
        for header_name in &self.remove {
            if let Ok(name) = HeaderName::from_str(header_name) {
                debug!("헤더 제거: {}", header_name);
                headers.remove(name.as_str());
            } else {
                error!("잘못된 헤더 이름: {}", header_name);
            }
        }

        // 2. set으로 덮어쓸 헤더 처리
        for (name, value) in &self.set {
            match (HeaderName::from_str(name), HeaderValue::from_str(value)) {
                (Ok(name), Ok(value)) => {
                    debug!("헤더 설정: {:?}={:?}", name, value);
                    headers.insert(name, value);
                }
                _ => error!("잘못된 헤더 설정: {}={}", name, value),
            }
        }

        // 3. 마지막으로 추가할 헤더 처리
        for (name, value) in &self.add {
            match (HeaderName::from_str(name), HeaderValue::from_str(value)) {
                (Ok(name), Ok(value)) => {
                    debug!("헤더 추가: {:?}={:?}", name, value);
                    headers.append(name, value);
                }
                _ => error!("잘못된 헤더 추가: {}={}", name, value),
            }
        }
        
        debug!("헤더 수정 완료. 최종 헤더: {:?}", headers);
    }
}

impl HeaderModification<Raw> {
    /// 새로운 HeaderModification 인스턴스 생성
    #[allow(dead_code)]
    pub fn new(
        add: HashMap<String, String>,
        remove: Vec<String>,
        set: HashMap<String, String>
    ) -> Self {
        Self {
            add,
            remove,
            set,
            _state: PhantomData,
        }
    }
}

impl Validatable<HeaderModification<Validated>> for HeaderModification<Raw> {
    type Error = MiddlewareConfigError;

    fn validate(self) -> Result<HeaderModification<Validated>, Self::Error> {
        let mut validator = MiddlewareValidator::new();
        validator.start_collecting();
        
        // 헤더 이름과 값 검증
        for (name, value) in &self.add {
            if let Err(msg) = validate_header_name(name) {
                validator.add_error(MiddlewareConfigError::InvalidValue {
                    field: format!("add.{}", name),
                    message: msg.to_string(),
                });
            }
            
            if HeaderValue::from_str(value).is_err() {
                validator.add_error(MiddlewareConfigError::InvalidValue {
                    field: format!("add.{}", name),
                    message: format!("헤더 값이 유효하지 않습니다: {}", value),
                });
            }
        }
        
        for (name, value) in &self.set {
            if let Err(msg) = validate_header_name(name) {
                validator.add_error(MiddlewareConfigError::InvalidValue {
                    field: format!("set.{}", name),
                    message: msg.to_string(),
                });
            }
            
            if HeaderValue::from_str(value).is_err() {
                validator.add_error(MiddlewareConfigError::InvalidValue {
                    field: format!("set.{}", name),
                    message: format!("헤더 값이 유효하지 않습니다: {}", value),
                });
            }
        }
        
        for name in &self.remove {
            if let Err(msg) = validate_header_name(name) {
                validator.add_error(MiddlewareConfigError::InvalidValue {
                    field: format!("remove.{}", name),
                    message: msg.to_string(),
                });
            }
        }
        
        if validator.has_errors() {
            let errors = validator.into_errors();
            Err(errors.into_iter().next().unwrap())
        } else {
            Ok(HeaderModification {
                add: self.add,
                remove: self.remove,
                set: self.set,
                _state: PhantomData,
            })
        }
    }
}

/// 헤더 미들웨어 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadersConfig<S: TypeState = Raw> {
    /// 요청 헤더 수정 설정
    #[serde(default)]
    pub request: HeaderModification<S>,
    
    /// 응답 헤더 수정 설정
    #[serde(default)]
    pub response: HeaderModification<S>,
    
    /// 타입스테이트 마커
    #[serde(skip)]
    _state: PhantomData<S>,
}

impl<S: TypeState> Default for HeadersConfig<S> {
    fn default() -> Self {
        Self {
            request: HeaderModification::default(),
            response: HeaderModification::default(),
            _state: PhantomData,
        }
    }
}

impl HeadersConfig<Raw> {
    /// 새로운 HeadersConfig 인스턴스 생성
    #[allow(dead_code)]
    pub fn new(
        request: HeaderModification<Raw>,
        response: HeaderModification<Raw>
    ) -> Self {
        Self {
            request,
            response,
            _state: PhantomData,
        }
    }

    pub fn from_flat_map(settings: &HashMap<String, String>) -> Result<Self, serde_json::Error> {
        let mut config = HeadersConfig::default();
        
        // headers.request.add.X-Key, headers_request_add_X-Key 등의 패턴을 처리
        for (key, value) in settings {
            debug!("헤더 설정 파싱: key={}, value={}", key, value);
            
            // 헤더 설정 키 분석
            if let Some((section, action, header_name)) = parse_header_key(key) {
                match (section.as_str(), action.as_str()) {
                    // 요청 헤더 처리
                    ("request", "add") => {
                        if let Ok(name) = validate_header_name(&header_name) {
                            config.request.add.insert(name, value.clone());
                        } else {
                            debug!("유효하지 않은 헤더 이름: {}", header_name);
                        }
                    },
                    ("request", "remove") => {
                        if let Ok(name) = validate_header_name(&header_name) {
                            config.request.remove.push(name);
                        } else {
                            debug!("유효하지 않은 헤더 이름: {}", header_name);
                        }
                    },
                    ("request", "set") => {
                        if let Ok(name) = validate_header_name(&header_name) {
                            config.request.set.insert(name, value.clone());
                        } else {
                            debug!("유효하지 않은 헤더 이름: {}", header_name);
                        }
                    },
                    
                    // 응답 헤더 처리
                    ("response", "add") => {
                        if let Ok(name) = validate_header_name(&header_name) {
                            config.response.add.insert(name, value.clone());
                        } else {
                            debug!("유효하지 않은 헤더 이름: {}", header_name);
                        }
                    },
                    ("response", "remove") => {
                        if let Ok(name) = validate_header_name(&header_name) {
                            config.response.remove.push(name);
                        } else {
                            debug!("유효하지 않은 헤더 이름: {}", header_name);
                        }
                    },
                    ("response", "set") => {
                        if let Ok(name) = validate_header_name(&header_name) {
                            config.response.set.insert(name, value.clone());
                        } else {
                            debug!("유효하지 않은 헤더 이름: {}", header_name);
                        }
                    },
                    _ => {}
                }
            }
            
            // 특별 패턴 처리 (헤더 이름이 값으로 지정된 경우)
            let key_lower = key.to_lowercase();
            if key_lower == "headers.request.remove" || key_lower == "headers_request_remove" {
                if let Ok(name) = validate_header_name(value) {
                    config.request.remove.push(name.clone());
                } else {
                    debug!("유효하지 않은 헤더 이름: {}", value);
                }
            } else if key_lower == "headers.response.remove" || key_lower == "headers_response_remove" {
                if let Ok(name) = validate_header_name(value) {
                    config.response.remove.push(name.clone());
                } else {
                    debug!("유효하지 않은 헤더 이름: {}", value);
                }
            }
        }
        
        debug!("최종 헤더 설정: {:?}", config);
        Ok(config)
    }
}

impl Validatable<HeadersConfig<Validated>> for HeadersConfig<Raw> {
    type Error = MiddlewareConfigError;

    fn validate(self) -> Result<HeadersConfig<Validated>, Self::Error> {
        // 요청 헤더 검증
        let validated_request = self.request.validate()?;
        
        // 응답 헤더 검증
        let validated_response = self.response.validate()?;
        
        Ok(HeadersConfig {
            request: validated_request,
            response: validated_response,
            _state: PhantomData,
        })
    }
}


/// 헤더 설정 키를 분석하여 (섹션, 액션, 헤더이름) 튜플을 반환합니다.
/// 예: "headers.request.add.X-Key" -> ("request", "add", "X-Key")
fn parse_header_key(key: &str) -> Option<(String, String, String)> {
    // 1. 점(.) 표기법: headers.request.add.X-Key 
    if key.contains('.') {
        let parts: Vec<&str> = key.split('.').collect();
        if parts.len() >= 4 && parts[0].eq_ignore_ascii_case("headers") {
            return Some((
                parts[1].to_lowercase(),
                parts[2].to_lowercase(),
                parts[3].to_string()
            ));
        }
    }
    // 2. 밑줄(_) 표기법: headers_request_add_X-Key
    else if key.contains('_') {
        let parts: Vec<&str> = key.split('_').collect();
        if parts.len() >= 4 && parts[0].eq_ignore_ascii_case("headers") {
            return Some((
                parts[1].to_lowercase(),
                parts[2].to_lowercase(),
                parts[3].to_string()
            ));
        }
    }
    
    None
}

/// HTTP 헤더 이름의 유효성을 검사합니다.
/// 유효한 경우 정규화된 헤더 이름을 반환합니다.
fn validate_header_name(name: &str) -> Result<String, &'static str> {
    // 빈 헤더 이름 검사
    if name.is_empty() {
        return Err("헤더 이름은 비어있을 수 없습니다");
    }
    
    // 헤더 이름에 유효하지 않은 문자가 있는지 검사
    if name.chars().any(|c| !c.is_ascii() || (c.is_ascii_whitespace() && c != ' ')) {
        return Err("헤더 이름에 유효하지 않은 문자가 포함되어 있습니다");
    }
    
    // 헤더 이름을 정규화하여 반환 (HTTP 헤더는 대소문자를 구분하지 않음)
    // 표준 헤더의 경우 일반적인 형식으로 변환 (Content-Type, User-Agent 등)
    Ok(name.to_string()) // 현재는 원래 형식 유지, 필요시 정규화 로직 추가
}

impl HeadersConfig<Validated> {
    /// Docker 라벨에서 설정을 파싱하고 곧바로 검증합니다.
    pub fn from_labels(labels: &HashMap<String, String>) -> Result<Self, MiddlewareError> {
        // 1. 유효성 검사: 특수한 오류 케이스를 미리 체크
        // 예: headers.request.add 또는 headers.response.add 패턴에서 빈 헤더 이름
        for (key, _) in labels {
            if key.contains("headers.request.add.") && key.ends_with('.') ||
               key.contains("headers.response.add.") && key.ends_with('.') {
                return Err(MiddlewareError::Config { 
                    message: "헤더 이름은 비어있을 수 없습니다".to_string()
                });
            }
        }
        
        // 2. 원시 설정 파싱
        let raw_config = HeadersConfig::<Raw>::from_flat_map(labels)
            .map_err(|e| MiddlewareError::Config { message: e.to_string() })?;
        
        // 3. 파싱된 설정 검증
        raw_config.validate()
            .map_err(|e| MiddlewareError::Config { message: e.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_from_flat_map_case_insensitive() {
        // 테스트 설정 (다양한 케이싱 스타일)
        let mut settings = HashMap::new();
        // 카멜 케이스
        settings.insert("headers.request.add.X-Request-ID".to_string(), "123".to_string());
        // 소문자
        settings.insert("headers.response.add.content-type".to_string(), "application/json".to_string());
        // 대문자
        settings.insert("HEADERS.RESPONSE.SET.SERVER".to_string(), "MyServer".to_string());
        // 스네이크 케이스
        settings.insert("headers_request_remove_authorization".to_string(), "".to_string());
        
        let config = HeadersConfig::from_flat_map(&settings).unwrap();
        
        // 카멜 케이스 설정 확인
        assert_eq!(config.request.add.get("X-Request-ID"), Some(&"123".to_string()));
        
        // 소문자 설정 확인
        assert_eq!(config.response.add.get("content-type"), Some(&"application/json".to_string()));
        
        // 대문자 설정 확인
        assert_eq!(config.response.set.get("SERVER"), Some(&"MyServer".to_string()));
        
        // 스네이크 케이스 설정 확인
        assert!(config.request.remove.contains(&"authorization".to_string()));
    }
    
    #[test]
    fn test_from_flat_map_basic() {
        // 기본 설정 테스트
        let mut settings = HashMap::new();
        settings.insert("headers.request.add.X-Test".to_string(), "test".to_string());
        settings.insert("headers.request.remove.Authorization".to_string(), "".to_string());
        settings.insert("headers.response.set.Server".to_string(), "TestServer".to_string());
        
        let config = HeadersConfig::from_flat_map(&settings).unwrap();
        
        assert_eq!(config.request.add.get("X-Test"), Some(&"test".to_string()));
        assert!(config.request.remove.contains(&"Authorization".to_string()));
        assert_eq!(config.response.set.get("Server"), Some(&"TestServer".to_string()));
    }
    
    #[test]
    fn test_header_modification_validate_success() {
        // 유효한 헤더로 설정
        let mut add = HashMap::new();
        add.insert("X-Test".to_string(), "test".to_string());
        
        let mut set = HashMap::new();
        set.insert("Content-Type".to_string(), "application/json".to_string());
        
        let remove = vec!["Authorization".to_string()];
        
        let header_mod = HeaderModification::<Raw>::new(add, remove, set);
        let validated = header_mod.validate();
        
        assert!(validated.is_ok());
    }
    
    #[test]
    fn test_header_modification_validate_invalid_header_name() {
        // 유효하지 않은 헤더 이름으로 설정
        let mut add = HashMap::new();
        add.insert("".to_string(), "test".to_string()); // 빈 헤더 이름
        
        let header_mod = HeaderModification::<Raw>::new(add, vec![], HashMap::new());
        let validated = header_mod.validate();
        
        assert!(validated.is_err());
        let err = validated.unwrap_err();
        match err {
            MiddlewareConfigError::InvalidValue { field, message } => {
                assert!(field.starts_with("add."));
                assert!(message.contains("비어있을 수 없습니다"));
            },
            _ => panic!("잘못된 오류 타입"),
        }
    }
    
    #[test]
    fn test_headers_config_validate_success() {
        // 유효한 설정으로 테스트
        let mut settings = HashMap::new();
        settings.insert("headers.request.add.X-Test".to_string(), "test".to_string());
        settings.insert("headers.response.set.Server".to_string(), "TestServer".to_string());
        
        let config = HeadersConfig::from_flat_map(&settings).unwrap();
        let validated = config.validate();
        
        assert!(validated.is_ok());
    }
    
    #[test]
    fn test_headers_config_validate_invalid_request() {
        // 요청 헤더에 유효하지 않은 값 설정
        let mut add = HashMap::new();
        add.insert("".to_string(), "test".to_string()); // 빈 헤더 이름
        
        let request = HeaderModification::<Raw>::new(add, vec![], HashMap::new());
        let response = HeaderModification::<Raw>::default();
        let config = HeadersConfig::<Raw>::new(request, response);
        
        let validated = config.validate();
        
        // 유효성 검사 실패 확인
        assert!(validated.is_err());
        
        if let Err(err) = validated {
            match err {
                MiddlewareConfigError::InvalidValue { field, message } => {
                    assert!(field.starts_with("add."));
                    assert!(message.contains("비어있을 수 없습니다"));
                },
                _ => panic!("잘못된 오류 타입"),
            }
        }
    }
    
    #[test]
    fn test_headers_config_from_flat_map_with_validation() {
        // 설정으로부터 검증된 설정 생성
        let mut settings = HashMap::new();
        settings.insert("headers.request.add.X-Test".to_string(), "test".to_string());
        settings.insert("headers.response.set.Server".to_string(), "TestServer".to_string());
        
        let config = HeadersConfig::from_flat_map(&settings).unwrap();
        let validated = config.validate();
        
        assert!(validated.is_ok());
        let validated_config = validated.unwrap();
        
        // 검증된 설정 확인
        assert_eq!(validated_config.request.add.get("X-Test"), Some(&"test".to_string()));
        assert_eq!(validated_config.response.set.get("Server"), Some(&"TestServer".to_string()));
    }
    
    #[test]
    fn test_headers_config_from_labels() {
        // Docker 라벨 형식으로 설정 테스트
        let mut labels = HashMap::new();
        labels.insert("headers.request.add.X-Request-ID".to_string(), "123".to_string());
        labels.insert("headers.response.set.Server".to_string(), "MyServer".to_string());
        
        let config = HeadersConfig::<Validated>::from_labels(&labels).unwrap();
        
        // 설정이 올바르게 파싱되었는지 확인
        assert_eq!(config.request.add.get("X-Request-ID"), Some(&"123".to_string()));
        assert_eq!(config.response.set.get("Server"), Some(&"MyServer".to_string()));
    }
    
    #[test]
    fn test_headers_config_from_labels_validation_failure() {
        // 유효하지 않은 설정으로 테스트 (빈 헤더 이름)
        let mut labels = HashMap::new();
        labels.insert("headers.request.add.".to_string(), "test".to_string()); // 빈 헤더 이름
        
        let result = HeadersConfig::<Validated>::from_labels(&labels);
        assert!(result.is_err());
        
        let err = result.unwrap_err();
        match err {
            MiddlewareError::Config { message } => {
                assert!(message.contains("비어있을 수 없습니다"));
            },
            _ => panic!("예상치 못한 오류 타입"),
        }
    }
} 