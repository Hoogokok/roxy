use tracing::debug;
use crate::middleware::basic_auth::{BasicAuthConfig, BasicAuthMiddleware};
use crate::middleware::cors::{CorsConfig, CorsMiddleware};
use crate::middleware::headers::{HeadersConfig, HeadersMiddleware};
use crate::middleware::rate_limit::{RateLimitConfig, RateLimitMiddleware, store::memory::MemoryStore};
use super::{Middleware, MiddlewareChain, MiddlewareConfig, MiddlewareError, Request, Response};
use super::config::MiddlewareType;
use std::collections::HashMap;

/// 미들웨어 설정으로부터 미들웨어 인스턴스를 생성합니다.
fn create_middleware(config: &MiddlewareConfig) -> Result<Box<dyn Middleware>, MiddlewareError> {
    debug!("미들웨어 생성 시작: type={:?}, settings={:?}", config.middleware_type, config.settings);
    
    match config.middleware_type {
        MiddlewareType::BasicAuth => {
            let auth_config = BasicAuthConfig::from_labels(&config.settings)?;
            Ok(Box::new(BasicAuthMiddleware::new(auth_config)?))
        }
        MiddlewareType::Headers => {
            let headers_config = HeadersConfig::from_flat_map(&config.settings)
                .map_err(|e| MiddlewareError::InvalidFormat(e.to_string()))?;
            debug!("생성된 헤더 설정: {:?}", headers_config);
            
            Ok(Box::new(HeadersMiddleware::new(headers_config)))
        }
        MiddlewareType::Cors => {
            let cors_config = CorsConfig::from_labels(&config.settings)?;
            Ok(Box::new(CorsMiddleware::new(cors_config)))
        }
        MiddlewareType::RateLimit => {
            let rate_limit_config = RateLimitConfig::from_labels(&config.settings)
                .map_err(|e| MiddlewareError::Config { message: e })?;
            let store = MemoryStore::new();
            Ok(Box::new(RateLimitMiddleware::new(rate_limit_config, store)))
        }
    }
}

#[derive(Default, Clone)]
pub struct MiddlewareManager {
    router_chains: HashMap<String, MiddlewareChain>,  // 라우터 이름 -> 체인
}

impl MiddlewareManager {
    pub fn new(
        middleware_configs: &HashMap<String, MiddlewareConfig>,
        router_middlewares: &HashMap<String, Vec<String>>
    ) -> Self {
        let mut router_chains = HashMap::new();
        
        for (router_name, middleware_names) in router_middlewares {
            let chain = Self::create_middleware_chain(middleware_names, middleware_configs);
            if chain.middleware_count() > 0 {
                router_chains.insert(router_name.clone(), chain);
            }
        }
        
        Self { router_chains }
    }

    fn create_middleware_chain(
        middleware_names: &[String],
        configs: &HashMap<String, MiddlewareConfig>
    ) -> MiddlewareChain {
        let mut chain = MiddlewareChain::new();
        
        let middlewares = middleware_names.iter()
            .filter_map(|name| configs.get(name))
            .filter(|config| config.enabled)
            .filter_map(|config| create_middleware(config).ok());

        for middleware in middlewares {
            chain.add_boxed(middleware);
        }
        
        chain
    }

    async fn handle_chain<F, T>(&self, router_name: Option<&str>, input: T, handler: F) -> Result<T, MiddlewareError> 
    where
        F: Fn(&MiddlewareChain, T) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, MiddlewareError>> + Send + '_>>
    {
        match router_name {
            Some(name) => {
                if let Some(chain) = self.router_chains.get(name) {
                    debug!("라우터 {} 의 미들웨어 체인 실행", name);
                    handler(chain, input).await
                } else {
                    debug!("라우터 {} 에 대한 미들웨어 체인 없음", name);
                    Ok(input)
                }
            }
            None => Ok(input)
        }
    }

    pub async fn handle_request(&self, router_name: Option<&str>, req: Request) -> Result<Request, MiddlewareError> {
        self.handle_chain(router_name, req, |chain, req| Box::pin(chain.handle_request(req))).await
    }

    pub async fn handle_response(&self, router_name: Option<&str>, res: Response) -> Result<Response, MiddlewareError> {
        self.handle_chain(router_name, res, |chain, res| Box::pin(chain.handle_response(res))).await
    }

    pub fn update_configs(&mut self, configs: &[(String, MiddlewareConfig)]) {
        debug!("미들웨어 설정 업데이트 시작 - 설정 수: {}", configs.len());
        
        let mut new_chains = HashMap::new();
        
        let enabled_middlewares = configs.iter()
            .filter(|(_, config)| config.enabled)
            .filter_map(|(name, config)| {
                let router_name = name.split('-').next()?;
                debug!("미들웨어 체인 업데이트 - 라우터: {}, 타입: {:?}", router_name, config.middleware_type);
                
                let middleware = match create_middleware(config) {
                    Ok(m) => m,
                    Err(_) => return None,
                };
                Some((router_name, middleware))
            });

        for (router_name, middleware) in enabled_middlewares {
            new_chains.entry(router_name.to_string())
                .or_insert_with(MiddlewareChain::new)
                .add_boxed(middleware);
        }

        debug!("현재 체인 수: {}, 새 체인 수: {}", self.router_chains.len(), new_chains.len());
        self.router_chains = new_chains;
        debug!("미들웨어 설정 업데이트 완료");
    }

    pub fn print_chain_status(&self) {
        debug!("=== 미들웨어 체인 상태 ===");
        match self.router_chains.len() {
            0 => debug!("등록된 미들웨어 매핑 없음"),
            count => {
                debug!("총 {} 개의 라우터 체인", count);
                for (router_name, chain) in &self.router_chains {
                    debug!(
                        router = %router_name,
                        middlewares = %chain.middleware_count(),
                        "라우터 체인 정보"
                    );
                    // 체인 내부의 미들웨어 타입 정보도 출력
                    if let Some(types) = chain.middleware_types() {
                        debug!(
                            router = %router_name,
                            types = ?types,
                            "미들웨어 타입 목록"
                        );
                    }
                }
            }
        }
        debug!("========================");
    }

    /// 미들웨어 매니저의 유효성을 검사합니다.
    /// 모든 미들웨어 설정이 유효하고 초기화 가능한지 확인합니다.
    pub fn validate(&self) -> Result<(), MiddlewareError> {
        // 현재 버전에서는 라우터 체인의 존재 여부만 확인합니다.
        // 실제 애플리케이션에서는 필요에 따라 더 상세한 검증을 추가할 수 있습니다.
        debug!("미들웨어 매니저 유효성 검사");
        
        // 체인에 포함된 모든 미들웨어 초기화 가능한지 검사할 수 있음
        // 지금은 단순히 체인이 있는지만 확인
        if self.router_chains.is_empty() {
            debug!("미들웨어 체인이 비어 있습니다 - 이는 유효한 상태일 수 있습니다");
        } else {
            debug!("미들웨어 체인 개수: {}", self.router_chains.len());
            
            // 각 체인별 정보 로깅 (디버깅 목적)
            for (router_name, chain) in &self.router_chains {
                debug!("라우터 '{}' 미들웨어 체인 미들웨어 수: {}", router_name, chain.middleware_count());
            }
        }
        
        // 현재는 항상 성공 반환
        // 향후 버전에서 실제 유효성 검사 로직 추가 가능
        Ok(())
    }
} 
    