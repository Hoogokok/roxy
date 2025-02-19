use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
        
        for (key, value) in labels {
            match key.split('.').collect::<Vec<_>>().as_slice() {
                ["cors", "allowOrigins"] => {
                    config.allow_origins = value.split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                },
                ["cors", "allowMethods"] => {
                    config.allow_methods = value.split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                },
                ["cors", "allowHeaders"] => {
                    config.allow_headers = value.split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                },
                ["cors", "exposeHeaders"] => {
                    config.expose_headers = value.split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                },
                ["cors", "maxAge"] => {
                    config.max_age = value.parse().ok();
                },
                ["cors", "allowCredentials"] => {
                    config.allow_credentials = value.parse().unwrap_or(false);
                },
                _ => continue,
            }
        }
        
        Ok(config)
    }
} 