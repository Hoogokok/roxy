use std::sync::Arc;
use tokio::net::TcpListener;
use hyper_util::rt::TokioIo;
use crate::server::error::Error;
use crate::settings::Settings;
use crate::tls::TlsConfig;
use tracing::{error, info};
use super::handler::RequestHandler;
use super::Result;

pub struct ServerListener {
    http_listener: TcpListener,
    https_config: Option<TlsConfig>,
}

impl ServerListener {
    pub async fn new(settings: &Settings) -> Result<Self> {
        // HTTP 리스너 초기화
        let http_listener = TcpListener::bind(format!("0.0.0.0:{}", settings.server.http_port))
            .await
            .map_err(|e| {
                error!(error = %e, port = settings.server.http_port, "HTTP 포트 바인딩 실패");
                e
            })?;

        info!(port = settings.server.http_port, "HTTP 리스너 시작");

        // HTTPS 설정 초기화
        let https_config = if settings.server.https_enabled {
            let cert_path = settings.server.tls_cert_path.as_ref()
                .ok_or_else(|| Error::ConfigError("TLS 인증서 경로가 설정되지 않음".into()))?;
            let key_path = settings.server.tls_key_path.as_ref()
                .ok_or_else(|| Error::ConfigError("TLS 키 경로가 설정되지 않음".into()))?;
            
            Some(TlsConfig::new(cert_path, key_path, settings.server.https_port)
                .await
                .map_err(|e| Error::Other(e))?)
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
        loop {
            tokio::select! {
                result = self.http_listener.accept() => {
                    match result {
                        Ok((stream, _)) => {
                            let handler = handler.clone();
                            tokio::spawn(async move {
                                let io = TokioIo::new(stream);
                                if let Err(err) = handler.handle_connection(io).await {
                                    error!(error = %err, "HTTP 연결 처리 실패");
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
                        Ok((stream, _)) => {
                            let handler = handler.clone();
                            let acceptor = self.https_config.as_ref().unwrap().acceptor.clone();
                            
                            tokio::spawn(async move {
                                match acceptor.accept(stream).await {
                                    Ok(tls_stream) => {
                                        let io = TokioIo::new(tls_stream);
                                        if let Err(err) = handler.handle_connection(io).await {
                                            error!(error = %err, "HTTPS 연결 처리 실패");
                                        }
                                    }
                                    Err(e) => {
                                        error!(error = %e, "TLS 핸드쉐이크 실패");
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