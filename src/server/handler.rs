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
        // 1. 요청 미들웨어 처리
        let req = match self.middleware_manager.handle_request(req).await {
            Ok(req) => req,
            Err(e) => return Ok(handle_middleware_error(e)),
        };

        // 2. 라우팅 및 프록시
        let table = self.routing_table.read().await;
        let response = match table.route_request(&req) {
            Ok(backend) => {
                match proxy::proxy_request(&self.proxy_config, backend, req).await {
                    Ok(response) => response,
                    Err(e) => {
                        error!(error = %e, "프록시 요청 실패");
                        proxy::error_response(&e)
                    }
                }
            }
            Err(e) => {
                error!(error = %e, "라우팅 실패");
                self.create_routing_error_response(e)
            }
        };

        // 3. 응답 미들웨어 처리
        match self.middleware_manager.handle_response(response).await {
            Ok(response) => Ok(response),
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