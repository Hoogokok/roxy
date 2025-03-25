use std::path::{Path, PathBuf};
use std::collections::HashMap;
use anyhow::{Result, Context};
use console::style;
use tokio::fs;
use futures_util::future::join_all;
use crate::docker;

/// 배치 처리 결과를 나타내는 구조체
#[derive(Debug)]
pub struct BatchResult {
    /// 총 처리 시도 개수
    pub total: usize,
    /// 성공한 개수
    pub success: usize,
    /// 실패한 개수
    pub failed: usize,
    /// 실패한 컨테이너 ID 목록
    pub failed_containers: Vec<String>,
}

/// 여러 컨테이너를 배치로 처리하는 명령어
pub async fn execute(
    container_ids: &[&str],
    prefix: &str,
    output_dir: impl AsRef<Path>,
    pretty: bool,
    use_example: bool
) -> Result<BatchResult> {
    execute_with_filename_pattern(
        container_ids,
        prefix, 
        output_dir,
        pretty,
        use_example,
        "{}.json"
    ).await
}

/// 여러 컨테이너를 배치로 처리하고 커스텀 파일명 패턴 적용
pub async fn execute_with_filename_pattern(
    container_ids: &[&str],
    prefix: &str,
    output_dir: impl AsRef<Path>,
    pretty: bool,
    use_example: bool,
    filename_pattern: &str
) -> Result<BatchResult> {
    let output_dir = output_dir.as_ref();
    
    // 출력 디렉토리 생성 (없는 경우)
    if !output_dir.exists() {
        fs::create_dir_all(output_dir).await
            .context(format!("출력 디렉토리를 생성할 수 없습니다: {}", output_dir.display()))?;
    }
    
    println!("{} 개의 컨테이너를 처리합니다...", style(container_ids.len()).cyan());
    
    let mut success = 0;
    let mut failed = 0;
    let mut failed_containers = Vec::new();
    
    let tasks = container_ids.iter().map(|&id| {
        let output_path = output_dir.join(filename_pattern.replace("{}", id));
        let prefix = prefix.to_string();
        let id_str = id.to_string();
        
        async move {
            let result = process_container(&id_str, &prefix, &output_path, pretty, use_example).await;
            (id_str, result)
        }
    });
    
    // 모든 작업 병렬 처리
    let results = join_all(tasks).await;
    
    // 결과 집계
    for (id, result) in results {
        match result {
            Ok(_) => {
                println!("{} {} 처리 완료", style("✓").green(), style(&id).green());
                success += 1;
            }
            Err(e) => {
                println!("{} {} 처리 실패: {}", style("✗").red(), style(&id).red(), e);
                failed += 1;
                failed_containers.push(id);
            }
        }
    }
    
    println!("\n{} 처리 결과:", style("배치 변환").cyan().bold());
    println!("  총 컨테이너: {}", style(container_ids.len()).cyan());
    println!("  성공: {}", style(success).green());
    println!("  실패: {}", style(failed).red());
    
    Ok(BatchResult {
        total: container_ids.len(),
        success,
        failed,
        failed_containers,
    })
}

/// 테스트용: 지정된 컨테이너를 실패 시뮬레이션하며 처리
pub async fn execute_with_error_simulation(
    container_ids: &[&str],
    prefix: &str,
    output_dir: impl AsRef<Path>,
    pretty: bool,
    use_example: bool,
    fail_ids: &[&str]
) -> Result<BatchResult> {
    let output_dir = output_dir.as_ref();
    
    // 출력 디렉토리 생성 (없는 경우)
    if !output_dir.exists() {
        fs::create_dir_all(output_dir).await
            .context(format!("출력 디렉토리를 생성할 수 없습니다: {}", output_dir.display()))?;
    }
    
    let mut success = 0;
    let mut failed = 0;
    let mut failed_containers = Vec::new();
    
    // 테스트용으로 실패할 컨테이너 처리
    for &id in container_ids {
        let output_path = output_dir.join(format!("{}.json", id));
        
        if fail_ids.contains(&id) {
            // 실패 시뮬레이션
            failed += 1;
            failed_containers.push(id.to_string());
        } else {
            // 정상 처리
            process_container(id, prefix, &output_path, pretty, use_example).await?;
            success += 1;
        }
    }
    
    Ok(BatchResult {
        total: container_ids.len(),
        success,
        failed,
        failed_containers,
    })
}

/// 단일 컨테이너 처리 함수
async fn process_container(
    container_id: &str,
    prefix: &str,
    output_path: &Path,
    pretty: bool,
    use_example: bool
) -> Result<()> {
    let labels = if use_example {
        // 예시 라벨 사용 (테스트용)
        docker::get_example_labels()
    } else {
        // Docker API를 사용하여 실제 컨테이너 라벨 조회
        docker::get_container_labels(container_id).await
            .context(format!("컨테이너 {} 라벨을 로드할 수 없습니다", container_id))?
    };
    
    // 라벨을 JSON으로 변환
    let json = roxy::settings::converter::labels_to_json(&labels, prefix);
    
    // 결과를 파일로 저장
    let json_str = if pretty {
        serde_json::to_string_pretty(&json)?
    } else {
        serde_json::to_string(&json)?
    };
    
    fs::write(output_path, json_str).await
        .context(format!("결과를 {}에 저장할 수 없습니다", output_path.display()))?;
    
    Ok(())
} 