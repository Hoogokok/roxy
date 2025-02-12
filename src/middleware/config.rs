use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 미들웨어 설정을 위한 공통 인터페이스
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum MiddlewareType {
    BasicAuth,
    Headers,
    // 추후 추가될 미들웨어 타입들...
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

fn default_enabled() -> bool {
    true
}

impl MiddlewareConfig {
    /// Docker 라벨에서 미들웨어 설정을 파싱합니다.
    pub fn from_labels(labels: &HashMap<String, String>) -> Vec<(String, Self)> {
        let mut configs = Vec::new();
        let prefix = "rproxy.http.middlewares.";
        let mut middleware_groups: HashMap<String, HashMap<String, String>> = HashMap::new();
        
        for (key, value) in labels {
            if let Some(rest) = key.strip_prefix(prefix) {
                let parts: Vec<&str> = rest.splitn(3, '.').collect();
                if parts.len() >= 2 {
                    let name = parts[0].to_string();
                    let settings = middleware_groups.entry(name).or_default();
                    if parts.len() == 2 {
                        settings.insert(parts[1].to_string(), value.clone());
                    } else {
                        settings.insert(format!("{}.{}", parts[1], parts[2]), value.clone());
                    }
                }
            }
        }

        for (name, settings) in middleware_groups {
            if let Some(type_str) = settings.get("type") {
                let middleware_type = match type_str.as_str() {
                    "basic-auth" => MiddlewareType::BasicAuth,
                    "headers" => MiddlewareType::Headers,
                    _ => continue,
                };

                let config = MiddlewareConfig {
                    middleware_type,
                    enabled: settings.get("enabled")
                        .map(|v| v.to_lowercase() == "true")
                        .unwrap_or(true),
                    order: settings.get("order")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0),
                    settings: settings.into_iter()
                        .filter(|(k, _)| k != "type" && k != "enabled" && k != "order")
                        .map(|(k, v)| (k, serde_json::Value::String(v)))
                        .collect(),
                };
                configs.push((name, config));
            }
        }

        configs
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

        let configs = MiddlewareConfig::from_labels(&labels);
        assert_eq!(configs.len(), 1);
        
        let (name, config) = &configs[0];
        assert_eq!(name, "my-headers");
        assert_eq!(config.middleware_type, MiddlewareType::Headers);
        assert!(config.enabled);
        assert_eq!(config.order, 0);
        assert!(config.settings.contains_key("headers.customResponseHeaders.X-Custom-Header"));
    }

    #[test]
    fn test_parse_toml_config() {
        let toml_str = r#"
            [middlewares.cors]
            middleware_type = "cors"
            enabled = true
            order = 1
            
            [middlewares.cors.settings]
            "headers.customResponseHeaders.X-Custom-Header" = "value"
        "#;

        let configs = MiddlewareConfig::from_toml(toml_str).unwrap();
        assert_eq!(configs.len(), 1);
        
        let config = configs.get("cors").unwrap();
        assert_eq!(config.middleware_type, MiddlewareType::Headers);
        assert!(config.enabled);
        assert_eq!(config.order, 1);
        assert!(config.settings.contains_key("headers.customResponseHeaders.X-Custom-Header"));
    }
} 