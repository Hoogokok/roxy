use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use tracing::debug;

/// 미들웨어 설정을 위한 공통 인터페이스
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum MiddlewareType {
    BasicAuth,
    Headers,
    // 추후 추가될 미들웨어 타입들...
}

impl FromStr for MiddlewareType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "headers" => Ok(MiddlewareType::Headers),
            "basic-auth" => Ok(MiddlewareType::BasicAuth),
            unknown => Err(format!("Unknown middleware type: {}", unknown)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiddlewareConfig {
    /// 미들웨어 타입
    pub middleware_type: MiddlewareType,
    
    /// 미들웨어 활성화 여부
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    
    /// 실행 순서 (낮은 숫자가 먼저 실행)
    #[serde(default)]
    pub order: i32,
    
    /// 미들웨어별 설정
    pub settings: HashMap<String, serde_json::Value>,
}

impl Default for MiddlewareConfig {
    fn default() -> Self {
        Self {
            middleware_type: MiddlewareType::Headers,  // 기본 타입
            enabled: default_enabled(),
            order: 0,
            settings: HashMap::new(),
        }
    }
}

fn default_enabled() -> bool {
    false
}

impl MiddlewareConfig {
    /// Docker 라벨에서 미들웨어 설정을 파싱합니다.
    pub fn from_labels(labels: &HashMap<String, String>) -> Result<Vec<(String, Self)>, String> {
        let mut configs = HashMap::new();
        
        for (key, value) in labels {
            if let Some(middleware_name) = key.strip_prefix("rproxy.http.middlewares.") {
                debug!("미들웨어 라벨 파싱: key={}, value={}", key, value);
                
                let parts: Vec<&str> = middleware_name.split('.').collect();
                if parts.len() < 2 {
                    continue;
                }

                let name = parts[0].to_string();
                let config = configs.entry(name.clone())
                    .or_insert_with(|| MiddlewareConfig::default());

                debug!("설정 추가: name={}, parts={:?}", name, parts);
                
                match parts[1] {
                    "type" => config.middleware_type = value.parse()?,
                    "enabled" => config.enabled = value.parse().unwrap_or(false),
                    _ => {
                        config.settings.insert(
                            parts[1..].join("."), 
                            serde_json::Value::String(value.clone())
                        );
                    }
                }
            }
        }

        debug!("최종 설정: {:?}", configs);
        Ok(configs.into_iter().collect())
    }

    /// TOML 설정에서 미들웨어 설정을 파싱합니다.
    pub fn from_toml(config: &str) -> Result<HashMap<String, Self>, toml::de::Error> {
        #[derive(Deserialize)]
        struct Config {
            middlewares: HashMap<String, MiddlewareConfig>,
        }

        let config: Config = toml::from_str(config)?;
        Ok(config.middlewares)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_docker_labels() {
        let mut labels = HashMap::new();
        labels.insert(
            "rproxy.http.middlewares.my-headers.type".to_string(),
            "headers".to_string(),
        );
        labels.insert(
            "rproxy.http.middlewares.my-headers.headers.customResponseHeaders.X-Custom-Header".to_string(),
            "value".to_string(),
        );

        let configs = MiddlewareConfig::from_labels(&labels).unwrap();
        assert_eq!(configs.len(), 1);
        
        let (name, config) = &configs[0];
        assert_eq!(name, "my-headers");
        assert_eq!(config.middleware_type, MiddlewareType::Headers);
        assert!(config.enabled);
        assert_eq!(config.order, 0);
        assert!(config.settings.contains_key("headers.customResponseHeaders.X-Custom-Header"));
    }
}