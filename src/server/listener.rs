use std::sync::Arc;
use tokio::net::TcpListener;
use hyper_util::rt::TokioIo;
use crate::server::error::Error;
use crate::settings::{Settings, HttpsEnabled};
use crate::tls::TlsConfig;
use tracing::{debug, error, info};
use super::handler::RequestHandler;
use super::Result;

pub struct ServerListener {
    http_listener: TcpListener,
    https_config: Option<TlsConfig>,
}

impl ServerListener {
    pub async fn new<HttpsState: Clone + Send + Sync + 'static>(
        settings: &Settings<HttpsState>
    ) -> Result<Self> {
        // HTTP 리스너 초기화
        let http_addr = format!("0.0.0.0:{}", settings.server.http_port());
        debug!("HTTP 리스너 바인딩 시작: {}", http_addr);
        let http_listener = TcpListener::bind(&http_addr)
            .await
            .map_err(|e| {
                error!(error = %e, addr = %http_addr, "HTTP 바인딩 실패");
                e
            })?;
        info!(addr = %http_addr, "HTTP 리스너 시작");

        // HTTPS 설정 초기화
        let https_config = if std::any::TypeId::of::<HttpsState>() == std::any::TypeId::of::<HttpsEnabled>() {
            debug!("HTTPS 설정 초기화 시작");
            let cert_path_opt = settings.server.tls_cert_path();
            let cert_path = cert_path_opt.as_deref()
                .expect("HTTPS 활성화되었지만 인증서 경로가 없음");
            
            let key_path_opt = settings.server.tls_key_path();
            let key_path = key_path_opt.as_deref()
                .expect("HTTPS 활성화되었지만 키 경로가 없음");
            
            debug!(
                cert_path = %cert_path,
                key_path = %key_path,
                "TLS 인증서 로드 시작"
            );

            // 기본값 사용 - 실제로는 서버 설정에서 가져와야 함
            let https_port = 443; 
            
            let config = TlsConfig::new(cert_path, key_path, https_port)
                .await
                .map_err(|e| {
                    error!(error = %e, "TLS 설정 초기화 실패");
                    Error::Server {
                        message: format!("TLS 설정 초기화 실패: {}", e),
                        source: None
                    }
                })?;
            
            Some(config)
        } else {
            None
        };

        Ok(Self {
            http_listener,
            https_config,
        })
    }

    pub async fn run(
        self,
        handler: Arc<RequestHandler>,
    ) -> Result<()> {
        info!("서버 리스너 시작");
        
        loop {
            tokio::select! {
                result = self.http_listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            debug!(addr = %addr, "새로운 HTTP 연결 수락");
                            let handler = handler.clone();
                            tokio::spawn(async move {
                                let io = TokioIo::new(stream);
                                if let Err(err) = handler.handle_connection(io).await {
                                    error!(error = %err, addr = %addr, "HTTP 연결 처리 실패");
                                }
                            });
                        }
                        Err(e) => {
                            error!(error = %e, "HTTP 연결 수락 실패");
                        }
                    }
                }
                
                result = async { 
                    if let Some(config) = &self.https_config {
                        config.listener.accept().await
                    } else {
                        std::future::pending().await
                    }
                } => {
                    match result {
                        Ok((stream, addr)) => {
                            debug!(addr = %addr, "새로운 HTTPS 연결 수락");
                            let handler = handler.clone();
                            let acceptor = self.https_config.as_ref().unwrap().acceptor.clone();
                            
                            tokio::spawn(async move {
                                match acceptor.accept(stream).await {
                                    Ok(tls_stream) => {
                                        debug!(addr = %addr, "TLS 핸드쉐이크 성공");
                                        let io = TokioIo::new(tls_stream);
                                        if let Err(err) = handler.handle_connection(io).await {
                                            error!(error = %err, addr = %addr, "HTTPS 연결 처리 실패");
                                        }
                                    }
                                    Err(e) => {
                                        error!(error = %e, addr = %addr, "TLS 핸드쉐이크 실패");
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            error!(error = %e, "HTTPS 연결 수락 실패");
                        }
                    }
                }
            }
        }
    }
} 