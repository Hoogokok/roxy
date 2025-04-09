use anyhow::Result;
use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

#[test]
fn test_cli_help_command() -> Result<()> {
    let mut cmd = Command::cargo_bin("roxy-cli")?;
    
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Docker 컨테이너 라벨을 Roxy 설정 파일로 변환하고 관리하는 도구입니다"))
        .stdout(predicate::str::contains("config"));
    
    Ok(())
}

#[test]
fn test_cli_version_command() -> Result<()> {
    let mut cmd = Command::cargo_bin("roxy-cli")?;
    
    cmd.arg("--version");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("roxy-cli"));
    
    Ok(())
}

#[test]
fn test_invalid_command() -> Result<()> {
    let mut cmd = Command::cargo_bin("roxy-cli")?;
    
    cmd.arg("invalid-command");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("error:"));
    
    Ok(())
}

#[test]
fn test_migrate_without_required_args() -> Result<()> {
    let mut cmd = Command::cargo_bin("roxy-cli")?;
    
    cmd.args(["config", "migrate"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("--output-dir <OUTPUT_DIR>"));
    
    Ok(())
}

#[test]
fn test_convert_without_container() -> Result<()> {
    let mut cmd = Command::cargo_bin("roxy-cli")?;
    
    cmd.args(["config", "convert"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("컨테이너 ID를 지정하거나 --example 플래그를 사용하세요"));
    
    Ok(())
}

#[test]
fn test_batch_convert_without_containers() -> Result<()> {
    let mut cmd = Command::cargo_bin("roxy-cli")?;
    
    // 출력 디렉토리만 지정하고 컨테이너 목록 없이 실행
    let temp_dir = tempdir()?;
    cmd.args([
        "config", "batch-convert",
        "--output-dir", temp_dir.path().to_str().unwrap(),
    ]);
    
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("--containers <CONTAINERS>"));
    
    Ok(())
}

#[test]
fn test_config_help_command() -> Result<()> {
    let mut cmd = Command::cargo_bin("roxy-cli")?;
    
    cmd.args(["config", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Docker 컨테이너 라벨과 JSON 설정 파일 간의 변환"))
        .stdout(predicate::str::contains("convert"))
        .stdout(predicate::str::contains("migrate"));
    
    Ok(())
}

#[test]
fn test_migrate_help_command() -> Result<()> {
    let mut cmd = Command::cargo_bin("roxy-cli")?;
    
    cmd.args(["config", "migrate", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Docker 컨테이너 라벨에서 JSON 설정 파일로 안전하게 마이그레이션"))
        .stdout(predicate::str::contains("--containers"))
        .stdout(predicate::str::contains("--all"))
        .stdout(predicate::str::contains("--output-dir"));
    
    Ok(())
} 