use hyper::header::{HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
        // 1. 헤더 삭제
        for name in &self.remove {
            headers.remove(name);
        }

        // 2. 헤더 추가 (기존 값 유지)
        for (name, value) in &self.add {
            if let (Ok(name), Ok(value)) = (
                name.parse::<HeaderName>(),
                HeaderValue::from_str(value)
            ) {
                headers.append(name, value);
            }
        }

        // 3. 헤더 설정 (기존 값 덮어쓰기)
        for (name, value) in &self.set {
            if let (Ok(name), Ok(value)) = (
                name.parse::<HeaderName>(),
                HeaderValue::from_str(value)
            ) {
                headers.insert(name, value);
            }
        }
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