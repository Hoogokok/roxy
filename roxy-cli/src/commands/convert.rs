use anyhow::{Result, Context};
use roxy::settings::converter::labels_to_json;
use std::path::PathBuf;
use console::style;
use crate::docker;

/// Docker 라벨을 JSON으로 변환하는 명령어 구현
pub async fn execute(
    container: &Option<String>,
    prefix: &str,
    output: &Option<PathBuf>,
    pretty: bool,
    example: bool
) -> Result<()> {
    let labels = if let Some(container_id) = container {
        // Docker API를 사용하여 실제 컨테이너 라벨 조회
        println!("{}", style("컨테이너에서 라벨 로드 중...").cyan());
        docker::get_container_labels(container_id).await
            .context("컨테이너에서 라벨을 로드할 수 없습니다")?
    } else if example {
        // 예시 라벨 사용
        println!("{}", style("예시 라벨 사용 중...").yellow());
        docker::get_example_labels()
    } else {
        return Err(anyhow::anyhow!("컨테이너 ID를 지정하거나 --example 플래그를 사용하세요"));
    };
    
    println!("{} 개의 라벨을 찾았습니다", style(labels.len()).green());
    
    // 라벨을 JSON으로 변환
    let json = labels_to_json(&labels, prefix);
    
    // 결과 출력 또는 파일로 저장
    let json_str = if pretty {
        serde_json::to_string_pretty(&json)?
    } else {
        serde_json::to_string(&json)?
    };
    
    if let Some(path) = output {
        std::fs::write(path, &json_str)
            .context(format!("{}에 결과를 저장할 수 없습니다", path.display()))?;
        println!("결과를 {}에 저장했습니다", style(path.display()).green());
    } else {
        println!("\n{}\n", json_str);
    }
    
    Ok(())
}
