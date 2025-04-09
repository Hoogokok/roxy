use anyhow::Result;
use tempfile::tempdir;
use roxy_cli::commands::migrate;
use roxy_cli::docker::{MockDockerClient, DockerClient};
use std::fs;
use std::collections::HashSet;
use roxy_cli::commands::migrate::{MigrationContext, Uninitialized};

#[tokio::test]
async fn test_migration_validation_report() -> Result<()> {
    // 임시 디렉토리 설정
    let temp_dir = tempdir()?;
    let output_dir = temp_dir.path().to_path_buf();
    
    // 테스트용 MockDockerClient 설정
    let mock_client = MockDockerClient::new();
    
    // 테스트용 컨테이너 ID 설정
    let containers = vec!["test-container".to_string()];
    
    // 검증 기능이 포함된 마이그레이션 실행
    let result = migrate::execute_with_validation(
        containers,
        output_dir.clone(),
        None,
        None,
        "roxy.http.".to_string(),
        "{}.json".to_string(),
        true, // 검증 활성화
        Box::new(mock_client) as Box<dyn DockerClient>
    ).await?;
    
    // 검증 결과 확인
    assert!(result.validation_report.is_some());
    let report = result.validation_report.unwrap();
    
    // 보고서에 필요한 정보가 포함되어 있는지 확인
    assert!(report.contains("검증 결과"));
    assert!(report.contains("성공한 파일"));
    assert!(report.contains("유효한 JSON"));
    
    // 생성된 파일 확인
    assert!(fs::metadata(output_dir.join("test-container.json")).is_ok());
    
    Ok(())
}

#[tokio::test]
async fn test_validation_errors_tracking() -> Result<()> {
    // 임시 디렉토리 설정
    let temp_dir = tempdir()?;
    let output_dir = temp_dir.path().to_path_buf();
    
    // 사용자 정의 컨테이너로 모의 클라이언트 생성
    let mut containers_map = std::collections::HashMap::new();
    
    // test-container 설정
    let mut test_container = std::collections::HashMap::new();
    test_container.insert(
        "roxy.http.middlewares.test-container-auth.type".to_string(),
        "basic-auth".to_string(),
    );
    containers_map.insert("test-container".to_string(), test_container);
    
    // non-existent-container는 추가하지 않아서 get_container_labels 호출 시 오류 발생
    
    // 커스텀 클라이언트 생성
    let docker_client = Box::new(MockDockerClient::with_containers(containers_map));
    
    // 실제 컨테이너와 존재하지 않는 컨테이너를 함께 설정
    let containers = vec!["test-container".to_string(), "non-existent-container".to_string()];
    
    // 마이그레이션 컨텍스트 생성
    let uninitialized = MigrationContext::<Uninitialized>::new(
        Some(containers),
        false,
        output_dir.clone(),
        None,
        None,
        "roxy.http.".to_string(),
        "{}.json".to_string(),
        true,
        false, // fail_fast: false
        false,
    ).await?;
    
    // 테스트용 Docker 클라이언트 설정
    let uninitialized = uninitialized.with_test_docker_client(docker_client);
    
    // 상태 전환: Uninitialized -> Initialized -> Validated
    let initialized = uninitialized.initialize()?;
    let validated = initialized.validate().await?;
    
    // 검증 오류 목록 확인 - non-existent-container에 대한 오류만 있어야 함
    let errors = validated.get_validation_errors();
    assert_eq!(errors.len(), 1, "존재하지 않는 컨테이너에 대한 오류가 있어야 함");
    assert!(errors[0].0 == "non-existent-container", "오류가 발생한 컨테이너 이름 확인");
    
    // 유효한 컨테이너 목록 확인
    let valid_containers = validated.get_valid_containers();
    assert_eq!(valid_containers.len(), 1, "유효한 컨테이너는 1개여야 함");
    assert_eq!(valid_containers[0], "test-container", "유효한 컨테이너는 test-container여야 함");
    
    // 백업 및 마이그레이션 진행
    let backed = validated.backup()?;
    let migrated = backed.migrate().await?;
    
    // 마이그레이션 결과 확인
    let result = migrated.get_migration_result().expect("마이그레이션 결과가 있어야 함");
    assert_eq!(result.total, 2, "총 컨테이너 수는 2개여야 함");
    assert_eq!(result.success, 1, "성공한 컨테이너는 1개여야 함");
    assert_eq!(result.failed, 1, "실패한 컨테이너는 1개여야 함");
    
    // 실패한 컨테이너 목록 확인
    let failed_containers: HashSet<_> = result.failed_containers.iter().cloned().collect();
    assert!(failed_containers.contains("non-existent-container"), "실패한 컨테이너 목록에 non-existent-container가 있어야 함");
    
    // 요약 정보에 실패한 컨테이너 정보가 포함되어 있는지 확인
    let summary = migrated.get_summary()?;
    assert!(summary.contains("non-existent-container"), "요약 정보에 실패한 컨테이너 정보가 포함되어야 함");
    
    // 출력 파일 확인
    let output_file = output_dir.join("test-container.json");
    assert!(output_file.exists(), "test-container.json 파일이 생성되어야 함");
    
    // 파일 내용 확인
    let file_content = fs::read_to_string(&output_file)?;
    assert!(file_content.contains("middlewares"), "JSON 파일에 미들웨어 정보가 포함되어야 함");
    
    Ok(())
}

#[tokio::test]
async fn test_fail_fast_behavior() -> Result<()> {
    // 임시 디렉토리 설정
    let temp_dir = tempdir()?;
    let output_dir = temp_dir.path().to_path_buf();
    
    // 존재하지 않는 컨테이너 설정
    let containers = vec!["non-existent-container".to_string()];
    
    // fail_fast를 true로 설정
    let uninitialized = MigrationContext::<Uninitialized>::new(
        Some(containers),
        false,
        output_dir.clone(),
        None,
        None,
        "roxy.http.".to_string(),
        "{}.json".to_string(),
        true,
        true, // fail_fast: true
        false,
    ).await?;
    
    // 테스트용 Docker 클라이언트 설정
    let docker_client = Box::new(MockDockerClient::new());
    let uninitialized = uninitialized.with_test_docker_client(docker_client);
    
    // 초기화
    let initialized = uninitialized.initialize()?;
    
    // 검증 단계에서 오류가 발생해야 함
    let validation_result = initialized.validate().await;
    assert!(validation_result.is_err(), "fail_fast가 true일 때 검증 실패가 즉시 오류를 반환해야 함");
    
    Ok(())
}

#[tokio::test]
async fn test_migration_result_report() -> Result<()> {
    // 임시 디렉토리 설정
    let temp_dir = tempdir()?;
    let output_dir = temp_dir.path().to_path_buf();
    
    // 사용자 정의 컨테이너로 모의 클라이언트 생성
    let mut containers_map = std::collections::HashMap::new();
    
    // test-container 설정
    let mut test_container = std::collections::HashMap::new();
    test_container.insert(
        "roxy.http.middlewares.test-container-auth.type".to_string(),
        "basic-auth".to_string(),
    );
    containers_map.insert("test-container".to_string(), test_container);
    
    // 커스텀 클라이언트 생성
    let docker_client = Box::new(MockDockerClient::with_containers(containers_map));
    
    // 마이그레이션 컨텍스트 생성
    let uninitialized = MigrationContext::<Uninitialized>::new(
        Some(vec!["test-container".to_string()]),
        false,
        output_dir.clone(),
        None,
        None,
        "roxy.http.".to_string(),
        "{}.json".to_string(),
        true,
        false,
        true, // auto_apply: true
    ).await?;
    
    // 테스트용 Docker 클라이언트 설정
    let uninitialized = uninitialized.with_test_docker_client(docker_client);
    
    // 마이그레이션 실행
    let initialized = uninitialized.initialize()?;
    let validated = initialized.validate().await?;
    let backed = validated.backup()?;
    let migrated = backed.migrate().await?;
    
    // auto_apply 기능 확인
    let migrated = migrated.apply_if_needed().await?;
    
    // 생성된 파일 확인
    let output_file = output_dir.join("test-container.json");
    assert!(output_file.exists(), "출력 파일이 생성되어야 함");
    
    // 파일 내용 확인
    println!("출력 파일 검증 중: {}", output_file.display());
    if output_file.exists() {
        let content = fs::read_to_string(&output_file)?;
        println!("파일 내용: {}", content);
        assert!(!content.is_empty(), "JSON 파일이 비어있지 않아야 함");
        assert!(content.contains("middlewares"), "JSON 파일에 미들웨어 정보가 포함되어야 함");
    } else {
        panic!("파일이 생성되지 않았습니다.");
    }
    
    // 결과 확인
    let result = migrated.get_migration_result().expect("마이그레이션 결과가 있어야 함");
    assert_eq!(result.success, 1, "성공한 컨테이너는 1개여야 함");
    assert_eq!(result.failed, 0, "실패한 컨테이너는 0개여야 함");
    
    // is_success 메서드 확인
    assert!(migrated.is_success(), "모든 마이그레이션이 성공했으므로 is_success()는 true여야 함");
    
    Ok(())
}
