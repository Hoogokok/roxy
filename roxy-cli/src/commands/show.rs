use anyhow::Result;
use console::style;

/// 현재 설정 정보 조회 명령어 구현
pub fn execute(format: &str, pretty: bool) -> Result<()> {
    // 현재는 간단한 정보만 표시
    println!("형식: {}, 예쁘게 출력: {}", format, pretty);
    println!("{}", style("이 기능은 아직 구현되지 않았습니다").yellow());
    
    // TODO: 실제 설정 정보 조회 및 표시 구현
    Ok(())
}
