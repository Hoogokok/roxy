use std::sync::Arc;
use tokio::sync::RwLock;
use hyper::{Request, Response, StatusCode};
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use crate::{
    routing_v2::{RoutingTable, RoutingError},
    middleware::{MiddlewareManager, handle_middleware_error},
    proxy::{self, ProxyConfig},
};
use tracing::error;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use tracing::debug;


pub struct RequestHandler {
    routing_table: Arc<RwLock<RoutingTable>>,
    middleware_manager: MiddlewareManager,
    proxy_config: ProxyConfig,
}

impl RequestHandler {
    pub fn new(
        routing_table: Arc<RwLock<RoutingTable>>,
        middleware_manager: MiddlewareManager,
    ) -> Self {
        Self {
            routing_table,
            middleware_manager,
            proxy_config: ProxyConfig::new(),
        }
    }

    pub async fn handle_request(
        &self,
        req: Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
        // 1. 라우팅
        let table = self.routing_table.read().await;
        let backend = match table.route_request(&req) {
            Ok(backend) => backend,
            Err(e) => {
                error!(error = %e, "라우팅 실패");
                return Ok(self.create_routing_error_response(e));
            }
        };

        // 2. 요청 미들웨어 처리 - 라우터 이름 로깅 추가
        debug!("미들웨어 처리 시작 - 라우터: {:?}", backend.router_name);
        let req = match self.middleware_manager
            .handle_request(backend.router_name.as_deref(), req).await 
        {
            Ok(req) => req,
            Err(e) => {
                error!(error = %e, "요청 미들웨어 처리 실패");
                return Ok(handle_middleware_error(e));
            }
        };

        // 3. 프록시 요청
        let response = match proxy::proxy_request(&self.proxy_config, backend, req).await {
            Ok(response) => response,
            Err(e) => {
                error!(error = %e, "프록시 요청 실패");
                return Ok(proxy::error_response(&e));
            }
        };

        // 4. 응답 미들웨어 처리 - 상세 로깅 추가
        debug!("응답 미들웨어 처리 시작 - 라우터: {:?}", backend.router_name);
        match self.middleware_manager
            .handle_response(backend.router_name.as_deref(), response).await 
        {
            Ok(response) => {
                debug!("응답 미들웨어 처리 완료 - 최종 헤더: {:?}", response.headers());
                Ok(response)
            }
            Err(e) => {
                error!(error = %e, "응답 미들웨어 처리 실패");
                Ok(handle_middleware_error(e))
            }
        }
    }

    fn create_routing_error_response(&self, error: RoutingError) -> Response<Full<Bytes>> {
        let status = match error {
            RoutingError::MissingHost | 
            RoutingError::InvalidHost { .. } | 
            RoutingError::InvalidPort { .. } | 
            RoutingError::HeaderParseError { .. } => StatusCode::BAD_REQUEST,
            RoutingError::BackendNotFound { .. } => StatusCode::NOT_FOUND,
            RoutingError::InvalidPathPattern { .. } => StatusCode::NOT_FOUND,
        };
        
        Response::builder()
            .status(status)
            .body(Full::new(Bytes::from(format!("Error: {}", error))))
            .unwrap_or_else(|e| {
                error!(error = %e, "에러 응답 생성 실패");
                Response::new(Full::new(Bytes::from("Internal Server Error")))
            })
    }

    pub async fn handle_connection<I>(&self, io: I) -> std::result::Result<(), Box<dyn std::error::Error>>
    where
        I: hyper::rt::Read + hyper::rt::Write + Send + Unpin + 'static,
    {
        http1::Builder::new()
            .serve_connection(
                io,
                service_fn(|req| self.handle_request(req)),
            )
            .await
            .map_err(|e| e.into())
    }
} 