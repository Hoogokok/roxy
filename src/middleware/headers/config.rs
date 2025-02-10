use hyper::header::{HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

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
} 