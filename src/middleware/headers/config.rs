use hyper::header::{HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error};
use std::str::FromStr;

/// 헤더 수정 작업 설정
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HeaderModification {
    /// 추가할 헤더
    #[serde(default)]
    pub add: HashMap<String, String>,
    
    /// 삭제할 헤더
    #[serde(default)]
    pub remove: Vec<String>,
    
    /// 덮어쓸 헤더
    #[serde(default)]
    pub set: HashMap<String, String>,
}

impl HeaderModification {
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

/// 헤더 미들웨어 설정
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HeadersConfig {
    /// 요청 헤더 수정 설정
    #[serde(default)]
    pub request: HeaderModification,
    
    /// 응답 헤더 수정 설정
    #[serde(default)]
    pub response: HeaderModification,
}

impl HeadersConfig {
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
} 