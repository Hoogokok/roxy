use anyhow::Result;
use clap::Parser;

mod cli;
mod commands;
mod docker;

use cli::{Cli, Commands, ConfigCommands};

#[tokio::main]
async fn main() -> Result<()> {
    // 로깅 설정
    tracing_subscriber::fmt::init();
    
    // CLI 파싱
    let cli = Cli::parse();
    
    // 명령어 실행
    match &cli.command {
        Commands::Config { subcommand } => {
            match subcommand {
                ConfigCommands::Convert { container, prefix, output, pretty, example } => {
                    commands::convert::execute(container, prefix, output, *pretty, *example).await?;
                },
                ConfigCommands::BatchConvert { containers, prefix, output_dir, pretty, filename_pattern, all } => {
                    // 모든 컨테이너 처리 옵션이 활성화된 경우
                    let container_ids = if *all {
                        // 모든 실행 중인 컨테이너 ID 가져오기
                        let all_containers = docker::get_all_running_containers().await?;
                        all_containers
                    } else {
                        // 제공된 컨테이너 목록 사용
                        containers.clone()
                    };
                    
                    // Vec<String>을 Vec<&str>로 변환
                    let container_id_refs: Vec<&str> = container_ids.iter()
                        .map(|s| s.as_str())
                        .collect();
                    
                    // 배치 변환 수행
                    commands::batch::execute_with_filename_pattern(
                        &container_id_refs,
                        prefix,
                        output_dir,
                        *pretty,
                        false, // 예시 라벨은 사용하지 않음 (실제 컨테이너 처리)
                        filename_pattern
                    ).await?;
                },
                ConfigCommands::Validate { file } => {
                    commands::validate::execute(file)?;
                },
                ConfigCommands::Show { format, pretty } => {
                    commands::show::execute(format, *pretty)?;
                },
                ConfigCommands::Migrate { containers, all, output_dir, backup_dir, log_file, prefix, filename_pattern, pretty, fail_fast, auto_apply } => {
                    commands::migrate::execute(containers.clone(), *all, output_dir.clone(), backup_dir.clone(), log_file.clone(), prefix.clone(), filename_pattern.clone(), *pretty, *fail_fast, *auto_apply).await?;
                }
            }
        }
    }
    
    Ok(())
} 