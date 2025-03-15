pub mod logging;
mod server;
mod tls;
mod error;
pub mod docker;
pub mod json;
pub mod watcher;
pub mod converter;
pub mod types;
pub mod schema;
pub mod parser;
mod raw;
pub mod typestate;

// 새로 분리한 모듈들
pub mod core;
mod loader;
mod merger;
mod deserializer;
mod load_balancer;

// 주요 타입과 함수를 내보냅니다
pub use server::{ServerSettings, parse_env_var, Either, HttpsDisabled, HttpsEnabled};
pub use logging::LogSettings;
pub use tls::TlsSettings;
pub use docker::DockerSettings;
pub use error::SettingsError;
pub use json::JsonConfig;
pub use parser::ValidatedConfig;
pub use raw::RawSettings;
pub use core::{Settings, Result};

// 테스트 코드는 core, loader, merger 각 모듈로 이동했습니다
