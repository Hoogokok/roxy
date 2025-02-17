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
    chains: HashMap<String, MiddlewareChain>,
    // 라우터 이름과 미들웨어 이름의 매핑
    router_middleware_map: HashMap<String, Vec<String>>,
}

impl MiddlewareManager {
    pub fn new(middleware_configs: &HashMap<String, MiddlewareConfig>) -> Self {
        let mut chains = HashMap::new();
        let mut router_middleware_map = HashMap::new();
        
        debug!("미들웨어 매니저 초기화 시작 - 설정 수: {}", middleware_configs.len());
        
        for (name, config) in middleware_configs {
            if config.enabled {
                // 미들웨어 이름에서 라우터 이름 추출 (예: "api-headers" -> "api")
                if let Some(router_name) = name.split('-').next() {
                    debug!("라우터 {} 에 미들웨어 {} 매핑", router_name, name);
                    router_middleware_map
                        .entry(router_name.to_string())
                        .or_insert_with(Vec::new)
                        .push(name.clone());
                }

                debug!("미들웨어 체인 생성 시도 - 라우터: {}, 타입: {:?}", name, config.middleware_type);
                let mut chain = MiddlewareChain::new();
                if let Ok(middleware) = create_middleware(config) {
                    chain.add_boxed(middleware);
                    chains.insert(name.clone(), chain);
                    debug!("미들웨어 체인 생성 완료 - 라우터: {}", name);
                }
            }
        }

        debug!("미들웨어 매니저 초기화 완료 - 체인 수: {}", chains.len());
        Self { chains, router_middleware_map }
    }

    pub async fn handle_request(&self, router_name: Option<&str>, req: Request) -> Result<Request, MiddlewareError> {
        match router_name {
            Some(name) => {
                if let Some(middleware_names) = self.router_middleware_map.get(name) {
                    debug!("라우터 {} 의 미들웨어 체인 실행 - 미들웨어 수: {}", name, middleware_names.len());
                    let mut current_req = req;
                    for middleware_name in middleware_names {
                        if let Some(chain) = self.chains.get(middleware_name) {
                            debug!("미들웨어 {} 실행", middleware_name);
                            current_req = chain.handle_request(current_req).await?;
                        }
                    }
                    Ok(current_req)
                } else {
                    debug!("라우터 {} 에 대한 미들웨어 매핑 없음", name);
                    Ok(req)
                }
            }
            None => {
                debug!("라우터 이름 없음 - 미들웨어 처리 생략");
                Ok(req)
            }
        }
    }

    pub async fn handle_response(&self, router_name: Option<&str>, res: Response) -> Result<Response, MiddlewareError> {
        match router_name {
            Some(name) => {
                if let Some(middleware_names) = self.router_middleware_map.get(name) {
                    debug!("라우터 {} 의 응답 미들웨어 체인 실행 - 미들웨어 수: {}", name, middleware_names.len());
                    let mut current_res = res;
                    // 응답은 역순으로 처리
                    for middleware_name in middleware_names.iter().rev() {
                        if let Some(chain) = self.chains.get(middleware_name) {
                            debug!("미들웨어 {} 실행", middleware_name);
                            current_res = chain.handle_response(current_res).await?;
                        }
                    }
                    Ok(current_res)
                } else {
                    debug!("라우터 {} 에 대한 미들웨어 매핑 없음", name);
                    Ok(res)
                }
            }
            None => {
                debug!("라우터 이름 없음 - 미들웨어 처리 생략");
                Ok(res)
            }
        }
    }

    pub fn update_configs(&mut self, configs: &[(String, MiddlewareConfig)]) {
        let mut new_chains = HashMap::new();
        
        debug!("미들웨어 설정 업데이트 시작 - 설정 수: {}", configs.len());
        
        for (name, config) in configs {
            if config.enabled {
                debug!("미들웨어 체인 업데이트 - 라우터: {}, 타입: {:?}", name, config.middleware_type);
                let mut chain = MiddlewareChain::new();
                if let Ok(middleware) = create_middleware(config) {
                    chain.add_boxed(middleware);
                    new_chains.insert(name.clone(), chain);
                }
            }
        }

        debug!("현재 체인 수: {}, 새 체인 수: {}", self.chains.len(), new_chains.len());
        self.chains = new_chains;
        debug!("미들웨어 설정 업데이트 완료");
    }

    pub fn print_chain_status(&self) {
        debug!("=== 미들웨어 체인 상태 ===");
        if self.router_middleware_map.is_empty() {
            debug!("등록된 미들웨어 매핑 없음");
        } else {
            for (router_name, middleware_names) in &self.router_middleware_map {
                debug!("라우터: {} - 미들웨어: {:?}", router_name, middleware_names);
                for middleware_name in middleware_names {
                    if let Some(chain) = self.chains.get(middleware_name) {
                        debug!("  - {}: {} 개의 미들웨어", middleware_name, chain.middleware_count());
                    }
                }
            }
        }
        debug!("========================");
    }
} 