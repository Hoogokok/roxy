use clap::{Parser, Subcommand};
use anyhow::{Result, Context};
use reverse_proxy_traefik::settings::json::JsonConfig;
use reverse_proxy_traefik::settings::converter::labels_to_json;
use std::collections::HashMap;
use std::path::PathBuf;
use console::style;

// CLI 앱 정의
#[derive(Parser)]
#[command(author, version, about = "Roxy - 리버스 프록시 설정 관리 도구", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

// 서브커맨드 정의
#[derive(Subcommand)]
enum Commands {
    /// 설정 명령어
    Config {
        #[command(subcommand)]
        subcommand: ConfigCommands,
    },
}

// 설정 관련 서브커맨드
#[derive(Subcommand)]
enum ConfigCommands {
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

// 메인 함수
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
                    convert_command(container, prefix, output, *pretty, *example).await?;
                },
                ConfigCommands::Validate { file } => {
                    validate_command(file)?;
                },
                ConfigCommands::Show { format, pretty } => {
                    show_command(format, *pretty)?;
                },
            }
        }
    }
    
    Ok(())
}

// Docker 라벨을 JSON으로 변환하는 명령어 구현
async fn convert_command(
    container: &Option<String>,
    prefix: &str,
    output: &Option<PathBuf>,
    pretty: bool,
    example: bool
) -> Result<()> {
    let labels = if let Some(container_id) = container {
        // Docker API를 사용하여 실제 컨테이너 라벨 조회
        println!("{}", style("컨테이너에서 라벨 로드 중...").cyan());
        get_container_labels(container_id).await
            .context("컨테이너에서 라벨을 로드할 수 없습니다")?
    } else if example {
        // 예시 라벨 사용
        println!("{}", style("예시 라벨 사용 중...").yellow());
        get_example_labels()
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

// 설정 파일 유효성 검사 명령어 구현
fn validate_command(file: &PathBuf) -> Result<()> {
    println!("{} 검증 중...", style(file.display()).cyan());
    
    // JsonConfig::from_file_strongly_typed를 사용하여 유효성 검사 수행
    match JsonConfig::from_file_strongly_typed(file) {
        Ok(_) => {
            println!("{} 설정 파일이 유효합니다", style("✓").green());
            Ok(())
        },
        Err(e) => {
            println!("{} 설정 파일이 유효하지 않습니다: {}", style("✗").red(), e);
            Err(anyhow::anyhow!("설정 파일 검증 실패"))
        }
    }
}

// 현재 설정 정보 조회 명령어 구현
fn show_command(format: &str, pretty: bool) -> Result<()> {
    // 현재는 간단한 정보만 표시
    println!("형식: {}, 예쁘게 출력: {}", format, pretty);
    println!("{}", style("이 기능은 아직 구현되지 않았습니다").yellow());
    
    // TODO: 실제 설정 정보 조회 및 표시 구현
    Ok(())
}

// 실제 컨테이너 라벨 가져오기
async fn get_container_labels(container_id: &str) -> Result<HashMap<String, String>> {
    let docker = bollard::Docker::connect_with_local_defaults()
        .context("Docker 데몬에 연결할 수 없습니다")?;
    
    let container = docker.inspect_container(container_id, None).await
        .context(format!("컨테이너 {}를 찾을 수 없습니다", container_id))?;
    
    // 컨테이너 라벨 추출
    let labels = container.config
        .and_then(|config| config.labels)
        .unwrap_or_default();
    
    Ok(labels)
}

// 예시 라벨 가져오기
fn get_example_labels() -> HashMap<String, String> {
    let mut labels = HashMap::new();
    
    // 미들웨어 예시
    labels.insert("roxy.http.middlewares.api-cors.type".to_string(), "cors".to_string());
    labels.insert("roxy.http.middlewares.api-cors.cors.allowOrigins".to_string(), 
                "http://localhost:3000,https://example.com".to_string());
    labels.insert("roxy.http.middlewares.api-cors.cors.allowMethods".to_string(), 
                "GET,POST,PUT,DELETE,OPTIONS".to_string());
    labels.insert("roxy.http.middlewares.api-cors.enabled".to_string(), "true".to_string());
    
    // 라우터 예시
    labels.insert("roxy.http.routers.api.rule".to_string(), 
                "Host(`test.localhost`) && PathPrefix(`/api`)".to_string());
    labels.insert("roxy.http.routers.api.middlewares".to_string(), "api-cors".to_string());
    labels.insert("roxy.http.routers.api.service".to_string(), "api".to_string());
    
    // 서비스 예시
    labels.insert("roxy.http.services.api.loadbalancer.server.port".to_string(), "80".to_string());
    
    labels
} 