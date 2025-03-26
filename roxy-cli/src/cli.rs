use clap::{Parser, Subcommand};
use std::path::PathBuf;

// CLI 앱 정의
#[derive(Parser)]
#[command(
    author, 
    version, 
    about = "Roxy - 리버스 프록시 설정 관리 도구", 
    long_about = "Roxy는 Docker 컨테이너 라벨을 Roxy 설정 파일로 변환하고 관리하는 도구입니다. \
                 Docker 라벨 기반 설정과 JSON 파일 기반 설정 간의 변환을 쉽게 수행할 수 있습니다."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

// 서브커맨드 정의
#[derive(Subcommand)]
pub enum Commands {
    /// 설정 관련 명령어
    #[command(
        about = "설정 명령어",
        long_about = "Docker 컨테이너 라벨과 JSON 설정 파일 간의 변환, 검증 및 관리를 위한 명령어 그룹입니다."
    )]
    Config {
        #[command(subcommand)]
        subcommand: ConfigCommands,
    },
}

// 설정 관련 서브커맨드
#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Docker 라벨을 JSON 설정으로 변환
    #[command(
        about = "Docker 라벨을 JSON 설정으로 변환",
        long_about = "단일 Docker 컨테이너의 라벨을 JSON 설정 파일로 변환합니다. \
                     주어진 컨테이너 ID 또는 이름의 라벨을 추출하고 Roxy 구성에 맞는 JSON으로 변환합니다."
    )]
    Convert {
        /// 컨테이너 ID 또는 이름
        #[arg(
            long, 
            short, 
            help = "처리할 Docker 컨테이너의 ID 또는 이름",
            long_help = "변환할 라벨 정보를 가져올 Docker 컨테이너의 ID 또는 이름입니다. \
                        Docker 명령어로 조회 가능한 컨테이너를, ID(전체 또는 일부) 또는 이름으로 지정할 수 있습니다."
        )]
        container: Option<String>,
        
        /// 라벨 접두사 (기본값: roxy.http.)
        #[arg(
            long, 
            default_value = "roxy.http.",
            help = "처리할 Docker 라벨의 접두사",
            long_help = "처리할 Docker 라벨의 접두사입니다. 지정한 접두사로 시작하는 라벨만 처리됩니다. \
                        일반적으로 'roxy.http.'로 시작하는 라벨을 처리합니다."
        )]
        prefix: String,
        
        /// 출력 파일 경로
        #[arg(
            long, 
            short,
            help = "변환 결과를 저장할 파일 경로",
            long_help = "변환된 JSON 설정을 저장할 파일 경로입니다. \
                        지정하지 않으면 표준 출력(stdout)으로 결과가 출력됩니다."
        )]
        output: Option<PathBuf>,
        
        /// 예쁘게 포맷팅
        #[arg(
            long, 
            short, 
            default_value = "true",
            help = "JSON 출력을 정형화(들여쓰기 적용)",
            long_help = "JSON 출력을 읽기 쉽게 정형화(들여쓰기 적용)합니다. \
                        false로 설정하면 한 줄로 압축된 JSON이 생성됩니다."
        )]
        pretty: bool,
        
        /// 예시 라벨 사용 (컨테이너 ID가 없을 때)
        #[arg(
            long,
            help = "예시 라벨을 사용하여 변환 (컨테이너 ID가 없을 때)",
            long_help = "컨테이너 ID를 지정하지 않았을 때 예시 라벨을 사용하여 변환합니다. \
                        이 옵션은 도구의 사용법을 익히거나 출력 형식을 확인하는 데 유용합니다."
        )]
        example: bool,
    },
    
    /// 여러 컨테이너의 Docker 라벨을 JSON 설정으로 일괄 변환
    #[command(
        about = "여러 컨테이너의 Docker 라벨을 JSON 설정으로 일괄 변환",
        long_about = "여러 Docker 컨테이너의 라벨을 JSON 설정 파일로 일괄 변환합니다. \
                     각 컨테이너마다 별도의 JSON 파일이 생성됩니다."
    )]
    BatchConvert {
        /// 컨테이너 ID 또는 이름 목록 (쉼표로 구분)
        #[arg(
            long, 
            short, 
            required = true, 
            value_delimiter = ',',
            help = "처리할 Docker 컨테이너 ID 또는 이름 목록 (쉼표로 구분)",
            long_help = "변환할 라벨 정보를 가져올 Docker 컨테이너의 ID 또는 이름 목록입니다. \
                        여러 컨테이너를 쉼표로 구분하여 지정할 수 있습니다. \
                        예: container1,container2,0123456789ab"
        )]
        containers: Vec<String>,
        
        /// 라벨 접두사 (기본값: roxy.http.)
        #[arg(
            long, 
            default_value = "roxy.http.",
            help = "처리할 Docker 라벨의 접두사",
            long_help = "처리할 Docker 라벨의 접두사입니다. 지정한 접두사로 시작하는 라벨만 처리됩니다. \
                        일반적으로 'roxy.http.'로 시작하는 라벨을 처리합니다."
        )]
        prefix: String,
        
        /// 출력 디렉토리 경로 (필수)
        #[arg(
            long, 
            short, 
            required = true,
            help = "변환 결과를 저장할 디렉토리 경로",
            long_help = "변환된 JSON 설정 파일을 저장할 디렉토리 경로입니다. \
                        디렉토리가 존재하지 않으면 자동으로 생성됩니다."
        )]
        output_dir: PathBuf,
        
        /// 예쁘게 포맷팅
        #[arg(
            long, 
            short, 
            default_value = "true",
            help = "JSON 출력을 정형화(들여쓰기 적용)",
            long_help = "JSON 출력을 읽기 쉽게 정형화(들여쓰기 적용)합니다. \
                        false로 설정하면 한 줄로 압축된 JSON이 생성됩니다."
        )]
        pretty: bool,
        
        /// 파일명 패턴 (기본값: {}.json, {}는 컨테이너 ID로 대체됨)
        #[arg(
            long, 
            default_value = "{}.json",
            help = "출력 파일명 패턴 ({}는 컨테이너 ID로 대체)",
            long_help = "출력 파일의 이름 패턴입니다. {}는 컨테이너 ID로 대체됩니다. \
                        예: {}.json은 container1.json, container2.json 등으로 변환됩니다."
        )]
        filename_pattern: String,
        
        /// 모든 실행 중인 컨테이너 포함
        #[arg(
            long,
            help = "모든 실행 중인 컨테이너를 대상으로 처리",
            long_help = "현재 실행 중인 모든 Docker 컨테이너를 대상으로 처리합니다. \
                        이 옵션이 지정되면 --containers 옵션은 무시됩니다."
        )]
        all: bool,
    },
    
    /// 설정 파일 유효성 검사
    #[command(
        about = "설정 파일 유효성 검사",
        long_about = "JSON 설정 파일의 유효성을 검사합니다. \
                     파일 구문과 Roxy 설정 스키마에 맞는지 확인합니다."
    )]
    Validate {
        /// 검사할 JSON 설정 파일 경로
        #[arg(
            required = true,
            help = "검사할 JSON 설정 파일 경로",
            long_help = "유효성을 검사할 JSON 설정 파일의 경로입니다. \
                        파일이 존재하지 않거나 JSON 형식이 아니면 오류가 발생합니다."
        )]
        file: PathBuf,
    },
    
    /// 현재 설정 정보 조회
    #[command(
        about = "현재 설정 정보 조회",
        long_about = "현재 적용 중인 설정 정보를 조회합니다. \
                     Docker 컨테이너 라벨이나 설정 파일의 내용을 확인할 수 있습니다."
    )]
    Show {
        /// 출력 형식 (json, yaml)
        #[arg(
            long, 
            short, 
            default_value = "json",
            help = "출력 형식 (json, yaml)",
            long_help = "설정 정보의 출력 형식입니다. 'json' 또는 'yaml'을 지정할 수 있습니다."
        )]
        format: String,
        
        /// 예쁘게 포맷팅
        #[arg(
            long, 
            short, 
            default_value = "true",
            help = "출력을 정형화(들여쓰기 적용)",
            long_help = "출력을 읽기 쉽게 정형화(들여쓰기 적용)합니다. \
                        false로 설정하면 한 줄로 압축된 출력이 생성됩니다."
        )]
        pretty: bool,
    },

    /// Docker 라벨에서 JSON 설정으로 마이그레이션
    #[command(
        about = "Docker 라벨에서 JSON 설정으로 마이그레이션",
        long_about = "Docker 컨테이너 라벨에서 JSON 설정 파일로 안전하게 마이그레이션합니다. \
                     기존 설정의 백업, 유효성 검사 및 타입 안전한 단계별 마이그레이션을 제공합니다."
    )]
    Migrate {
        /// 마이그레이션할 컨테이너 ID 또는 이름 목록 (쉼표로 구분)
        #[arg(
            long, 
            short, 
            value_delimiter = ',',
            help = "마이그레이션할 컨테이너 ID 또는 이름 목록 (쉼표로 구분)",
            long_help = "마이그레이션할 Docker 컨테이너의 ID 또는 이름 목록입니다. \
                        여러 컨테이너를 쉼표로 구분하여 지정할 수 있습니다. \
                        --all 옵션과 함께 사용할 수 없습니다."
        )]
        containers: Option<Vec<String>>,
        
        /// 모든 실행 중인 컨테이너 마이그레이션
        #[arg(
            long, 
            short,
            help = "모든 실행 중인 컨테이너를 대상으로 마이그레이션",
            long_help = "현재 실행 중인 모든 Docker 컨테이너를 대상으로 마이그레이션합니다. \
                        이 옵션이 지정되면 --containers 옵션은 무시됩니다."
        )]
        all: bool,
        
        /// 출력 디렉토리 경로
        #[arg(
            long, 
            short, 
            required = true,
            help = "마이그레이션 결과를 저장할 디렉토리 경로",
            long_help = "마이그레이션된 JSON 설정 파일을 저장할 디렉토리 경로입니다. \
                        디렉토리가 존재하지 않으면 자동으로 생성됩니다."
        )]
        output_dir: PathBuf,
        
        /// 라벨 접두사 (기본값: roxy.http.)
        #[arg(
            long, 
            default_value = "roxy.http.",
            help = "처리할 Docker 라벨의 접두사",
            long_help = "처리할 Docker 라벨의 접두사입니다. 지정한 접두사로 시작하는 라벨만 처리됩니다. \
                        일반적으로 'roxy.http.'로 시작하는 라벨을 처리합니다."
        )]
        prefix: String,
        
        /// 백업 디렉토리 경로 (기본값: {output_dir}/backups)
        #[arg(
            long,
            help = "기존 설정 파일의 백업을 저장할 디렉토리 경로",
            long_help = "기존 설정 파일의 백업을 저장할 디렉토리 경로입니다. \
                        지정하지 않으면 출력 디렉토리 내의 'backups' 폴더가 사용됩니다."
        )]
        backup_dir: Option<PathBuf>,
        
        /// 로그 파일 경로 (기본값: {output_dir}/migration.log)
        #[arg(
            long,
            help = "마이그레이션 로그를 저장할 파일 경로",
            long_help = "마이그레이션 과정의 로그를 저장할 파일 경로입니다. \
                        지정하지 않으면 출력 디렉토리 내의 'migration.log' 파일이 사용됩니다."
        )]
        log_file: Option<PathBuf>,
        
        /// 예쁘게 포맷팅
        #[arg(
            long, 
            short, 
            default_value = "true",
            help = "JSON 출력을 정형화(들여쓰기 적용)",
            long_help = "JSON 출력을 읽기 쉽게 정형화(들여쓰기 적용)합니다. \
                        false로 설정하면 한 줄로 압축된 JSON이 생성됩니다."
        )]
        pretty: bool,
        
        /// 파일명 패턴 (기본값: {}.json, {}는 컨테이너 ID로 대체됨)
        #[arg(
            long, 
            default_value = "{}.json",
            help = "출력 파일명 패턴 ({}는 컨테이너 ID로 대체)",
            long_help = "출력 파일의 이름 패턴입니다. {}는 컨테이너 ID로 대체됩니다. \
                        예: {}.json은 container1.json, container2.json 등으로 변환됩니다."
        )]
        filename_pattern: String,
        
        /// 검증 실패 시 중단
        #[arg(
            long, 
            default_value = "true",
            help = "검증 실패 시 마이그레이션 프로세스 중단",
            long_help = "검증 단계에서 오류가 발생하면 마이그레이션 프로세스를 즉시 중단합니다. \
                        false로 설정하면 오류가 발생해도 가능한 한 많은 컨테이너를 처리합니다."
        )]
        fail_fast: bool,
        
        /// 자동으로 설정 파일 적용
        #[arg(
            long,
            help = "마이그레이션 후 자동으로 설정 파일 적용",
            long_help = "마이그레이션이 성공적으로 완료된 후 자동으로 새 설정 파일을 적용합니다. \
                        이 옵션은 시스템에 따라 Roxy 서비스 재시작 등의 작업을 수행할 수 있습니다."
        )]
        auto_apply: bool,
        
        /// 생성된 JSON 설정 파일 검증
        #[arg(
            short = 'v', 
            long,
            help = "마이그레이션 후 생성된 JSON 설정 파일 검증",
            long_help = "마이그레이션 후 생성된 JSON 설정 파일의 유효성을 검사합니다. \
                        파일 구문과 Roxy 설정 스키마에 맞는지 확인합니다."
        )]
        validate: bool,
        
        /// 상세 로깅 활성화
        #[arg(
            short = 'd', 
            long,
            help = "상세한 로깅 정보 출력",
            long_help = "마이그레이션 과정에서 더 상세한 로깅 정보를 출력합니다. \
                        문제 해결이나 디버깅에 유용합니다."
        )]
        detailed_logging: bool,
    },
}
