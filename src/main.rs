mod docker;
mod proxy;
mod config;
mod logging;
mod tls;
mod routing_v2;
mod middleware;
mod settings;
mod server;

use tracing::info;
use crate::config::Config;
use crate::server::ServerManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 설정 로드
    let config = Config::load()?;
    
    // 2. 로깅 초기화
    logging::init_logging(&config.logging)?;
    
    // 3. 서버 매니저 생성 및 실행
    let server = ServerManager::new(config).await?;
    info!("서버 시작");
    
    // 4. 서버 실행
    server.run().await?;
    
    Ok(())
}
