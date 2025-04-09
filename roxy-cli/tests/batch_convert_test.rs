use tempfile::tempdir;
use anyhow::Result;

// 배치 변환 기능을 테스트합니다
#[tokio::test]
async fn test_batch_convert() -> Result<()> {
    // 임시 디렉토리 생성
    let temp_dir = tempdir()?;
    let output_dir = temp_dir.path();
    
    // 테스트할 컨테이너 ID 목록
    let container_ids = vec!["container1", "container2", "container3"];
    
    // 예상 결과 파일들
    let expected_files = container_ids.iter()
        .map(|id| output_dir.join(format!("{}.json", id)))
        .collect::<Vec<_>>();
    
    // 배치 변환 실행
    let result = roxy_cli::commands::batch::execute(
        &container_ids,
        "roxy.http.",
        output_dir,
        true, // pretty
        true // 테스트용으로 예시 라벨 사용
    ).await?;
    
    // 결과 확인
    assert_eq!(result.total, 3);
    assert_eq!(result.success, 3);
    assert_eq!(result.failed, 0);
    
    // 결과 파일 확인
    for file_path in &expected_files {
        assert!(file_path.exists(), "결과 파일이 생성되지 않았습니다: {:?}", file_path);
        
        // 파일 내용 확인 (기본 구조만)
        let content = std::fs::read_to_string(file_path)?;
        assert!(content.contains("\"middlewares\""));
        assert!(content.contains("\"routers\""));
        assert!(content.contains("\"services\""));
    }
    
    Ok(())
}

// 일부 컨테이너가 실패하는 경우 테스트
#[tokio::test]
async fn test_batch_convert_with_failures() -> Result<()> {
    // 임시 디렉토리 생성
    let temp_dir = tempdir()?;
    let output_dir = temp_dir.path();
    
    // 테스트할 컨테이너 ID 목록 (비정상 ID 포함)
    let container_ids = vec!["valid1", "invalid1", "valid2"];
    
    // 실패할 컨테이너 ID 설정
    let fail_ids = vec!["invalid1"];
    
    // 배치 변환 실행 (실패할 컨테이너 ID 지정)
    let result = roxy_cli::commands::batch::execute_with_error_simulation(
        &container_ids,
        "roxy.http.",
        output_dir,
        true, // pretty
        true, // 실제 컨테이너 대신 예시 라벨 사용 (true로 변경)
        &fail_ids
    ).await?;
    
    // 결과 확인
    assert_eq!(result.total, 3);
    assert_eq!(result.success, 2);
    assert_eq!(result.failed, 1);
    
    // 성공한 컨테이너만 결과 파일 확인
    assert!(output_dir.join("valid1.json").exists());
    assert!(output_dir.join("valid2.json").exists());
    assert!(!output_dir.join("invalid1.json").exists());
    
    // 실패 컨테이너 목록 확인
    assert_eq!(result.failed_containers.len(), 1);
    assert!(result.failed_containers.contains(&"invalid1".to_string()));
    
    Ok(())
}

// 모든 출력 파일이 지정된 디렉토리에 생성되는지 테스트
#[tokio::test]
async fn test_batch_convert_output_directory() -> Result<()> {
    // 임시 디렉토리 생성
    let temp_dir = tempdir()?;
    let output_dir = temp_dir.path().join("nested").join("results");
    
    // 중첩 디렉토리가 아직 없음 (자동 생성 확인)
    assert!(!output_dir.exists());
    
    // 테스트할 컨테이너 ID 목록
    let container_ids = vec!["test1", "test2"];
    
    // 배치 변환 실행
    let result = roxy_cli::commands::batch::execute(
        &container_ids,
        "roxy.http.",
        &output_dir,
        true, // pretty
        true // 테스트용으로 예시 라벨 사용
    ).await?;
    
    // 결과 확인
    assert_eq!(result.success, 2);
    
    // 디렉토리가 생성되었는지 확인
    assert!(output_dir.exists());
    assert!(output_dir.is_dir());
    
    // 결과 파일 확인
    assert!(output_dir.join("test1.json").exists());
    assert!(output_dir.join("test2.json").exists());
    
    Ok(())
}

// 사용자 정의 파일 이름 패턴 테스트
#[tokio::test]
async fn test_batch_convert_custom_filename() -> Result<()> {
    // 임시 디렉토리 생성
    let temp_dir = tempdir()?;
    let output_dir = temp_dir.path();
    
    // 테스트할 컨테이너 ID 목록
    let container_ids = vec!["web", "api"];
    
    // 배치 변환 실행 (커스텀 파일명 패턴 사용)
    let result = roxy_cli::commands::batch::execute_with_filename_pattern(
        &container_ids,
        "roxy.http.",
        output_dir,
        true, // pretty
        true, // 테스트용으로 예시 라벨 사용
        "config_{}.json" // 커스텀 패턴
    ).await?;
    
    // 결과 확인
    assert_eq!(result.success, 2);
    
    // 결과 파일 확인 (커스텀 패턴 적용)
    assert!(output_dir.join("config_web.json").exists());
    assert!(output_dir.join("config_api.json").exists());
    
    Ok(())
}

// 중첩 디렉토리 자동 생성 테스트
#[tokio::test]
async fn test_nested_directory_creation() -> Result<()> {
    // 임시 디렉토리 생성
    let temp_dir = tempdir()?;
    let nested_dir = temp_dir.path().join("nested").join("results");
    
    // 중첩 디렉토리가 아직 없음 확인
    assert!(!nested_dir.exists());
    
    // 테스트할 컨테이너 ID 목록
    let container_ids = vec!["test1"];
    
    // 배치 변환 실행
    roxy_cli::commands::batch::execute(
        &container_ids,
        "roxy.http.",
        &nested_dir,
        true, // pretty
        true  // 테스트용으로 예시 라벨 사용
    ).await?;
    
    // 디렉토리 생성 확인
    assert!(nested_dir.exists());
    assert!(nested_dir.is_dir());
    
    // 결과 파일 확인
    assert!(nested_dir.join("test1.json").exists());
    
    Ok(())
} 