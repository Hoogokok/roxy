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
    chain: MiddlewareChain,
}

impl MiddlewareManager {
    pub fn new(middleware_configs: &HashMap<String, MiddlewareConfig>) -> Self {
        let mut chain = MiddlewareChain::new();
        
        // 정렬을 위해 Vec으로 변환
        let mut ordered_configs: Vec<_> = middleware_configs.iter()
            .filter(|(_, config)| config.enabled)
            .collect();
        ordered_configs.sort_by_key(|(_, config)| config.order);

        // 미들웨어 생성 및 체인에 추가
        for (_, config) in ordered_configs {
            match create_middleware(config) {
                Ok(middleware) => chain.add_boxed(middleware),
                Err(e) => {
                    error!("미들웨어 생성 실패: {}", e);
                    continue;
                }
            }
        }

        Self { chain }  // middlewares 필드는 제거
    }

    pub async fn handle_request(&self, req: Request) -> Result<Request, MiddlewareError> {
        self.chain.handle_request(req).await
    }

    pub async fn handle_response(&self, res: Response) -> Result<Response, MiddlewareError> {
        self.chain.handle_response(res).await
    }

    pub fn update_configs(&mut self, configs: &[(String, MiddlewareConfig)]) {
        let mut chain = MiddlewareChain::new();
        
        // 정렬을 위해 Vec으로 변환
        let mut ordered_configs: Vec<_> = configs.iter()
            .filter(|(_, config)| config.enabled)
            .collect();
        ordered_configs.sort_by_key(|(_, config)| config.order);

        // 미들웨어 생성 및 체인에 추가
        for (_, config) in ordered_configs {
            match create_middleware(config) {
                Ok(middleware) => chain.add_boxed(middleware),
                Err(e) => {
                    error!("미들웨어 생성 실패: {}", e);
                    continue;
                }
            }
        }

        self.chain = chain;
    }
} 