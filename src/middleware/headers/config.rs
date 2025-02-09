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