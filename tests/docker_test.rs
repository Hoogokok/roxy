use reverse_proxy_traefik::docker::{DockerManager, DockerError};
use reverse_proxy_traefik::config::Config;
use tracing::warn;

#[tokio::test]
async fn test_docker_manager_initialization() {
    // 테스트용 기본 설정
    let config = Config {
        label_prefix: "reverse-proxy.".to_string(),
        docker_network: "reverse-proxy-network".to_string(),
        http_port: 8080,
        https_enabled: false,
        https_port: 443,
        tls_cert_path: None,
        tls_key_path: None,
        logging: Default::default(),
    };
    
    match DockerManager::new(config).await {
        Ok(_) => {
            println!("Docker 매니저 초기화 성공");
        },
        Err(e) => {
            // CI 환경이나 Docker가 실행되지 않은 환경에서는 연결 실패가 예상됨
            warn!("Docker 데몬 연결 실패 (예상된 결과일 수 있음): {}", e);
            assert!(matches!(e, DockerError::ConnectionError { .. }));
        }
    }
}

#[tokio::test]
async fn test_get_container_routes() {
    // 기본 설정으로 DockerManager 생성
    let config = Config {
        label_prefix: "reverse-proxy.".to_string(),
        docker_network: "reverse-proxy-network".to_string(),
        http_port: 8080,
        https_enabled: false,
        https_port: 443,
        tls_cert_path: None,
        tls_key_path: None,
        logging: Default::default(),
    };
    
    let manager = DockerManager::new(config).await
        .expect("Docker 매니저 생성 실패");

    // 컨테이너 라우트 조회 테스트
    match manager.get_container_routes().await {
        Ok(routes) => {
            // 라우트가 있든 없든 성공적으로 조회되어야 함
            println!("조회된 라우트 수: {}", routes.len());
        },
        Err(e) => {
            // Docker 데몬 연결 실패가 아닌 경우에만 실패로 처리
            if !matches!(e, DockerError::ConnectionError { .. }) {
                panic!("컨테이너 라우트 조회 실패: {}", e);
            }
        }
    }
} 