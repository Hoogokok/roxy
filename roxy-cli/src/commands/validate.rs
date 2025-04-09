use anyhow::{Result, anyhow};
use console::style;
use roxy::settings::json::JsonConfig;
use std::path::PathBuf;

/// 설정 파일 유효성 검사 명령어 구현
pub fn execute(file: &PathBuf) -> Result<()> {
    println!("{} 검증 중...", style(file.display()).cyan());
    
    // JsonConfig::from_file_strongly_typed를 사용하여 유효성 검사 수행
    match JsonConfig::from_file_strongly_typed(file) {
        Ok(_) => {
            println!("{} 설정 파일이 유효합니다", style("✓").green());
            Ok(())
        },
        Err(e) => {
            println!("{} 설정 파일이 유효하지 않습니다: {}", style("✗").red(), e);
            Err(anyhow!("설정 파일 검증 실패"))
        }
    }
}
