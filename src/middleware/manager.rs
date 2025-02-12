use tracing::error;
use crate::middleware::basic_auth::{BasicAuthConfig, BasicAuthMiddleware, create_authenticator};
use crate::middleware::headers::{HeadersConfig, HeadersMiddleware};
use crate::config::Config;
use super::{Middleware, MiddlewareChain, MiddlewareConfig, MiddlewareError, Request, Response};
use super::config::MiddlewareType;

/// 미들웨어 설정으로부터 미들웨어 인스턴스를 생성합니다.
fn create_middleware(config: &MiddlewareConfig) -> Result<Box<dyn Middleware>, MiddlewareError> {
    match config.middleware_type {
        MiddlewareType::BasicAuth => {
            // settings를 BasicAuthConfig로 변환
            let auth_config: BasicAuthConfig = serde_json::from_value(
                serde_json::to_value(&config.settings)?
            )?;
            
            // 인증기 생성
            let authenticator = create_authenticator(&auth_config)?;
            
            // 미들웨어 생성
            Ok(Box::new(BasicAuthMiddleware::new(
                auth_config,
                authenticator,
            )))
        }
        MiddlewareType::Headers => {
            // settings를 HeadersConfig로 변환
            let headers_config: HeadersConfig = serde_json::from_value(
                serde_json::to_value(&config.settings)?
            )?;
            
            // 미들웨어 생성
            Ok(Box::new(HeadersMiddleware::new(
                headers_config,
            )))
        }
    }
}

pub struct MiddlewareManager {
    chain: MiddlewareChain,
}

impl MiddlewareManager {
    pub fn new(config: &Config) -> Self {
        let mut chain = MiddlewareChain::new();
        
        // 설정에서 미들웨어 로드 및 정렬
        let mut middlewares: Vec<_> = config.middlewares.iter()
            .filter(|(_, config)| config.enabled)
            .collect();
        middlewares.sort_by_key(|(_, config)| config.order);

        // 미들웨어 생성 및 체인에 추가
        for (_, config) in middlewares {
            match create_middleware(config) {
                Ok(middleware) => chain.add_boxed(middleware),
                Err(e) => {
                    error!("미들웨어 생성 실패: {}", e);
                    continue;
                }
            }
        }

        Self { chain }
    }

    pub async fn handle_request(&self, req: Request) -> Result<Request, MiddlewareError> {
        self.chain.handle_request(req).await
    }

    pub async fn handle_response(&self, res: Response) -> Result<Response, MiddlewareError> {
        self.chain.handle_response(res).await
    }
} 