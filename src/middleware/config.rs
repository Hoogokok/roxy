use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct MiddlewareConfig {
    pub enabled: bool,
    pub order: Option<i32>,
    #[serde(flatten)]
    pub settings: HashMap<String, serde_json::Value>,
}

impl MiddlewareConfig {
    pub fn from_labels(labels: &HashMap<String, String>) -> Vec<(String, Self)> {
        // Docker 라벨에서 미들웨어 설정 파싱
        // traefik.http.middlewares.<name>.<type>
        todo!()
    }

    pub fn from_toml(config: &str) -> Result<HashMap<String, Self>, toml::de::Error> {
        toml::from_str(config)
    }
}
