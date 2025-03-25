use std::collections::HashMap;
use tempfile::tempdir;
use roxy_cli::commands::convert;
use std::fs;

// 라벨 -> JSON 변환 테스트
#[tokio::test]
async fn test_convert_example_labels() -> anyhow::Result<()> {
    // 임시 디렉토리 생성
    let temp_dir = tempdir()?;
    let output_path = temp_dir.path().join("test_output.json");
    
    // 테스트 실행 - 예시 라벨로 JSON 변환
    let container = None;
    let prefix = "roxy.http.";
    let output = Some(output_path.clone());
    let pretty = true;
    let example = true;
    
    // convert 명령 실행
    convert::execute(&container, prefix, &output, pretty, example).await?;
    
    // 파일이 존재하는지 확인
    assert!(output_path.exists());
    
    // 파일 내용 읽기
    let content = fs::read_to_string(&output_path)?;
    
    // 예상 결과 확인
    assert!(content.contains("middlewares"));
    assert!(content.contains("api-cors"));
    assert!(content.contains("routers"));
    assert!(content.contains("services"));
    
    // 임시 디렉토리 정리는 자동으로 수행됨
    Ok(())
}

// 미들웨어 설정이 올바르게 변환되는지 테스트
#[test]
fn test_middleware_conversion() {
    // 테스트 라벨 설정
    let mut labels = HashMap::new();
    labels.insert("roxy.http.middlewares.test-auth.type".to_string(), "basicAuth".to_string());
    labels.insert("roxy.http.middlewares.test-auth.basicAuth.users".to_string(), "user:password".to_string());
    
    // 변환
    let json = roxy::settings::converter::labels_to_json(&labels, "roxy.http.");
    
    // 디버깅 출력
    println!("변환된 JSON: {:#?}", json);
    
    // JSON 문자열로 변환 후 파싱하여 확인
    let json_str = serde_json::to_string(&json).unwrap();
    println!("JSON 문자열: {}", json_str);
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    
    // 변환 결과 검증
    assert!(parsed.is_object());
    assert!(parsed["middlewares"].is_object());
    assert!(parsed["middlewares"]["test-auth"].is_object());
    assert_eq!(parsed["middlewares"]["test-auth"]["type"], "basicAuth");
    
    // settings 하위에 있는지 확인
    if parsed["middlewares"]["test-auth"]["settings"].is_object() {
        assert_eq!(parsed["middlewares"]["test-auth"]["settings"]["users"], "user:password");
    } else if parsed["middlewares"]["test-auth"]["basicAuth"].is_object() {
        assert_eq!(parsed["middlewares"]["test-auth"]["basicAuth"]["users"], "user:password");
    } else {
        panic!("users 필드를 찾을 수 없습니다.");
    }
}

// 라우터 설정이 올바르게 변환되는지 테스트
#[test]
fn test_router_conversion() {
    // 테스트 라벨 설정
    let mut labels = HashMap::new();
    labels.insert("roxy.http.routers.api.rule".to_string(), "Host(`api.test`)".to_string());
    labels.insert("roxy.http.routers.api.service".to_string(), "api-service".to_string());
    labels.insert("roxy.http.routers.api.middlewares".to_string(), "auth,cors".to_string());
    
    // 변환
    let json = roxy::settings::converter::labels_to_json(&labels, "roxy.http.");
    
    // JSON 문자열로 변환 후 파싱하여 확인
    let json_str = serde_json::to_string(&json).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    
    // 변환 결과 검증
    assert!(parsed.is_object());
    assert!(parsed["routers"].is_object());
    assert!(parsed["routers"]["api"].is_object());
    assert_eq!(parsed["routers"]["api"]["rule"], "Host(`api.test`)");
    assert_eq!(parsed["routers"]["api"]["service"], "api-service");
    
    // 미들웨어 배열 확인
    let middlewares = &parsed["routers"]["api"]["middlewares"];
    assert!(middlewares.is_array());
    assert_eq!(middlewares[0], "auth");
    assert_eq!(middlewares[1], "cors");
} 