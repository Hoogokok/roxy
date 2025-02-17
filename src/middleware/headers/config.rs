use hyper::header::{HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
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
    // TOML 파일에서 설정 로드
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        Ok(Self::from_toml(&content)?)
    }

    // TOML 문자열에서 설정 파싱
    pub fn from_toml(content: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(content)
    }

    /// Docker 레이블에서 설정을 파싱합니다.
    pub fn from_labels(labels: &HashMap<String, String>, name: &str) -> Option<Self> {
        let prefix = format!("rproxy.http.middlewares.{}.headers", name);
        
        let mut config = HeadersConfig::default();

        // 요청 헤더 파싱
        for (key, value) in labels {
            if key.starts_with(&format!("{}.request.add.", prefix)) {
                let header_name = key.strip_prefix(&format!("{}.request.add.", prefix))?;
                config.request.add.insert(header_name.to_string(), value.clone());
            } else if key.starts_with(&format!("{}.request.remove", prefix)) {
                if let Some(header) = value.split(',').next() {
                    config.request.remove.push(header.trim().to_string());
                }
            } else if key.starts_with(&format!("{}.request.set.", prefix)) {
                let header_name = key.strip_prefix(&format!("{}.request.set.", prefix))?;
                config.request.set.insert(header_name.to_string(), value.clone());
            }
        }

        // 응답 헤더 파싱
        for (key, value) in labels {
            if key.starts_with(&format!("{}.response.add.", prefix)) {
                let header_name = key.strip_prefix(&format!("{}.response.add.", prefix))?;
                config.response.add.insert(header_name.to_string(), value.clone());
            } else if key.starts_with(&format!("{}.response.remove", prefix)) {
                if let Some(header) = value.split(',').next() {
                    config.response.remove.push(header.trim().to_string());
                }
            } else if key.starts_with(&format!("{}.response.set.", prefix)) {
                let header_name = key.strip_prefix(&format!("{}.response.set.", prefix))?;
                config.response.set.insert(header_name.to_string(), value.clone());
            }
        }

        Some(config)
    }

    pub fn from_flat_map(settings: &HashMap<String, String>) -> Result<Self, serde_json::Error> {
        let mut config = HeadersConfig::default();
        
        for (key, value) in settings {
            debug!("헤더 설정 파싱: key={}, value={}", key, value);
            let parts: Vec<&str> = key.split('.').collect();
            match parts.as_slice() {
                ["headers", "request", "add", header_name] => {
                    config.request.add.insert(header_name.to_string(), value.clone());
                },
                ["headers", "response", "add", header_name] => {
                    config.response.add.insert(header_name.to_string(), value.clone());
                },
                ["headers", "response", "remove"] => {
                    debug!("헤더 제거 설정: {}", value);
                    config.response.remove.push(value.clone());
                },
                ["headers", "response", "set", header_name] => {
                    config.response.set.insert(header_name.to_string(), value.clone());
                },
                _ => continue,
            }
        }
        debug!("최종 헤더 설정: {:?}", config);
        Ok(config)
    }
} 