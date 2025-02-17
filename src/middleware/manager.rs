use tracing::error;
use tracing::debug;
use crate::middleware::basic_auth::{BasicAuthConfig, BasicAuthMiddleware, create_authenticator};
use crate::middleware::headers::{HeadersConfig, HeadersMiddleware};
use super::{Middleware, MiddlewareChain, MiddlewareConfig, MiddlewareError, Request, Response};
use super::config::MiddlewareType;
use std::collections::HashMap;

/// 미들웨어 설정으로부터 미들웨어 인스턴스를 생성합니다.
fn create_middleware(config: &MiddlewareConfig) -> Result<Box<dyn Middleware>, MiddlewareError> {
    debug!("미들웨어 생성 시작: type={:?}, settings={:?}", config.middleware_type, config.settings);
    
    match config.middleware_type {
        MiddlewareType::BasicAuth => {
            let auth_config: BasicAuthConfig = serde_json::from_value(
                serde_json::to_value(&config.settings)?
            )?;
            Ok(Box::new(BasicAuthMiddleware::new(auth_config)?))
        }
        MiddlewareType::Headers => {
            let string_settings: HashMap<String, String> = config.settings.iter()
                .map(|(k, v)| {
                    let string_value = v.as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| v.to_string());
                    (k.clone(), string_value)
                })
                .collect();
            debug!("변환된 설정: {:?}", string_settings);

            let headers_config = HeadersConfig::from_flat_map(&string_settings)
                .map_err(|e| MiddlewareError::InvalidFormat(e.to_string()))?;
            debug!("생성된 헤더 설정: {:?}", headers_config);
            
            Ok(Box::new(HeadersMiddleware::new(headers_config)))
        }
    }
}

#[derive(Default, Clone)]
pub struct MiddlewareManager {
    // 라우터 이름을 키로 사용하여 각각의 미들웨어 체인 관리
    chains: HashMap<String, MiddlewareChain>,
}

impl MiddlewareManager {
    pub fn new(middleware_configs: &HashMap<String, MiddlewareConfig>) -> Self {
        let mut chains = HashMap::new();
        
        // 라우터별로 미들웨어 체인 생성
        for (name, config) in middleware_configs {
            if config.enabled {
                let mut chain = MiddlewareChain::new();
                if let Ok(middleware) = create_middleware(config) {
                    chain.add_boxed(middleware);
                    chains.insert(name.clone(), chain);
                }
            }
        }

        Self { chains }
    }

    pub async fn handle_request(&self, router_name: Option<&str>, req: Request) -> Result<Request, MiddlewareError> {
        match router_name.and_then(|name| self.chains.get(name)) {
            Some(chain) => chain.handle_request(req).await,
            None => Ok(req)  // 미들웨어가 없으면 요청을 그대로 통과
        }
    }

    pub async fn handle_response(&self, router_name: Option<&str>, res: Response) -> Result<Response, MiddlewareError> {
        match router_name.and_then(|name| self.chains.get(name)) {
            Some(chain) => chain.handle_response(res).await,
            None => Ok(res)  // 미들웨어가 없으면 응답을 그대로 통과
        }
    }

    pub fn update_configs(&mut self, configs: &[(String, MiddlewareConfig)]) {
        let mut new_chains = HashMap::new();
        
        for (name, config) in configs {
            if config.enabled {
                let mut chain = MiddlewareChain::new();
                if let Ok(middleware) = create_middleware(config) {
                    chain.add_boxed(middleware);
                    new_chains.insert(name.clone(), chain);
                }
            }
        }

        self.chains = new_chains;
    }
} 