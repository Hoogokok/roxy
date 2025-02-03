use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_rustls::rustls::{self, Certificate, PrivateKey};
use tokio_rustls::TlsAcceptor;
use tracing::{error, info};

pub struct TlsConfig {
    pub acceptor: TlsAcceptor,
    pub listener: TcpListener,
}

impl TlsConfig {
    pub async fn new(cert_path: &str, key_path: &str, port: u16) -> Result<Self, Box<dyn std::error::Error>> {
        let tls_config = Self::load_tls_config(cert_path, key_path)?;
        let acceptor = TlsAcceptor::from(Arc::new(tls_config));
        
        let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await
            .map_err(|e| {
                error!(error = %e, port = port, "HTTPS 포트 바인딩 실패");
                e
            })?;

        info!(port = port, "HTTPS 리스너 시작");
        Ok(Self { acceptor, listener })
    }

    fn load_tls_config(cert_path: &str, key_path: &str) -> Result<rustls::ServerConfig, Box<dyn std::error::Error>> {
        let cert_file = File::open(cert_path)?;
        let mut cert_reader = BufReader::new(cert_file);
        let certs = rustls_pemfile::certs(&mut cert_reader)?
            .into_iter()
            .map(Certificate)
            .collect();

        let key_file = File::open(key_path)?;
        let mut key_reader = BufReader::new(key_file);
        let key = rustls_pemfile::pkcs8_private_keys(&mut key_reader)?
            .first()
            .ok_or("개인키를 찾을 수 없음")?
            .clone();

        let config = rustls::ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(certs, PrivateKey(key))?;

        Ok(config)
    }
} 