use clap::{Parser, Subcommand};
use std::path::PathBuf;

// CLI 앱 정의
#[derive(Parser)]
#[command(author, version, about = "Roxy - 리버스 프록시 설정 관리 도구", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

// 서브커맨드 정의
#[derive(Subcommand)]
pub enum Commands {
    /// 설정 명령어
    Config {
        #[command(subcommand)]
        subcommand: ConfigCommands,
    },
}

// 설정 관련 서브커맨드
#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Docker 라벨을 JSON 설정으로 변환
    Convert {
        /// 컨테이너 ID 또는 이름
        #[arg(long, short)]
        container: Option<String>,
        
        /// 라벨 접두사 (기본값: roxy.http.)
        #[arg(long, default_value = "roxy.http.")]
        prefix: String,
        
        /// 출력 파일 경로
        #[arg(long, short)]
        output: Option<PathBuf>,
        
        /// 예쁘게 포맷팅
        #[arg(long, short, default_value = "true")]
        pretty: bool,
        
        /// 예시 라벨 사용 (컨테이너 ID가 없을 때)
        #[arg(long)]
        example: bool,
    },
    
    /// 설정 파일 유효성 검사
    Validate {
        /// 검사할 JSON 설정 파일 경로
        #[arg(required = true)]
        file: PathBuf,
    },
    
    /// 현재 설정 정보 조회
    Show {
        /// 출력 형식 (json, yaml)
        #[arg(long, short, default_value = "json")]
        format: String,
        
        /// 예쁘게 포맷팅
        #[arg(long, short, default_value = "true")]
        pretty: bool,
    },
}
