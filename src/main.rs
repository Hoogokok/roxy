mod docker;
mod proxy;
mod logging;
mod tls;
mod routing_v2;
mod middleware;
mod settings;
mod server;

use settings::{HttpsDisabled, HttpsEnabled};
use tracing::info;
use crate::{
    settings::{Settings, Either},
    server::manager_v2::ServerManager as ServerManagerV2,
    server::ServerInterface,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Settings 로드
    let settings_either: Either<Settings<HttpsDisabled>, Settings<HttpsEnabled>> = Settings::<HttpsDisabled>::load().await?;
    
    // 로깅 초기화 (Either에서 logging 필드 추출)
    let logging_settings = match &settings_either {
        Either::Left(http_settings) => &http_settings.logging,
        Either::Right(https_settings) => &https_settings.logging,
    };
    logging::init_logging(logging_settings)?;
    
    info!("설정 로드 완료");
    
    // v2 서버 매니저 생성
    info!("서버 매니저 생성 중...");
    let mut server = ServerManagerV2::from_either(settings_either).await?;
    
    // 서버 실행
    info!("서버 시작");
    server.start().await?;
    
    Ok(())
}
