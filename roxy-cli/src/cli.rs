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
    
    /// 여러 컨테이너의 Docker 라벨을 JSON 설정으로 일괄 변환
    BatchConvert {
        /// 컨테이너 ID 또는 이름 목록 (쉼표로 구분)
        #[arg(long, short, required = true, value_delimiter = ',')]
        containers: Vec<String>,
        
        /// 라벨 접두사 (기본값: roxy.http.)
        #[arg(long, default_value = "roxy.http.")]
        prefix: String,
        
        /// 출력 디렉토리 경로 (필수)
        #[arg(long, short, required = true)]
        output_dir: PathBuf,
        
        /// 예쁘게 포맷팅
        #[arg(long, short, default_value = "true")]
        pretty: bool,
        
        /// 파일명 패턴 (기본값: {}.json, {}는 컨테이너 ID로 대체됨)
        #[arg(long, default_value = "{}.json")]
        filename_pattern: String,
        
        /// 모든 실행 중인 컨테이너 포함
        #[arg(long)]
        all: bool,
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

    /// Docker 라벨에서 JSON 설정으로 마이그레이션
    Migrate {
        /// 마이그레이션할 컨테이너 ID 또는 이름 목록 (쉼표로 구분)
        #[arg(long, short, value_delimiter = ',')]
        containers: Option<Vec<String>>,
        
        /// 모든 실행 중인 컨테이너 마이그레이션
        #[arg(long, short)]
        all: bool,
        
        /// 출력 디렉토리 경로
        #[arg(long, short, required = true)]
        output_dir: PathBuf,
        
        /// 라벨 접두사 (기본값: roxy.http.)
        #[arg(long, default_value = "roxy.http.")]
        prefix: String,
        
        /// 백업 디렉토리 경로 (기본값: {output_dir}/backups)
        #[arg(long)]
        backup_dir: Option<PathBuf>,
        
        /// 로그 파일 경로 (기본값: {output_dir}/migration.log)
        #[arg(long)]
        log_file: Option<PathBuf>,
        
        /// 예쁘게 포맷팅
        #[arg(long, short, default_value = "true")]
        pretty: bool,
        
        /// 파일명 패턴 (기본값: {}.json, {}는 컨테이너 ID로 대체됨)
        #[arg(long, default_value = "{}.json")]
        filename_pattern: String,
        
        /// 검증 실패 시 중단
        #[arg(long, default_value = "true")]
        fail_fast: bool,
        
        /// 자동으로 설정 파일 적용
        #[arg(long)]
        auto_apply: bool,
        
        /// 생성된 JSON 설정 파일 검증
        #[arg(short = 'v', long)]
        validate: bool,
        
        /// 상세 로깅 활성화
        #[arg(short = 'd', long)]
        detailed_logging: bool,
    },
}
