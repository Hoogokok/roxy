use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub docker_network: String,
    pub label_prefix: String,
    pub http_port: u16,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            docker_network: env::var("PROXY_DOCKER_NETWORK")
                .unwrap_or_else(|_| "reverse-proxy-network".to_string()),
            
            label_prefix: env::var("PROXY_LABEL_PREFIX")
                .unwrap_or_else(|_| "reverse-proxy.".to_string()),
            
            http_port: env::var("PROXY_HTTP_PORT")
                .unwrap_or_else(|_| "80".to_string())
                .parse()
                .unwrap_or(80),
        }
    }
} 