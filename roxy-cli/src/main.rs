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
                    commands::execute_convert(container, prefix, output, *pretty, *example).await?;
                },
                ConfigCommands::Validate { file } => {
                    commands::execute_validate(file)?;
                },
                ConfigCommands::Show { format, pretty } => {
                    commands::execute_show(format, *pretty)?;
                },
            }
        }
    }
    
    Ok(())
} 