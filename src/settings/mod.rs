mod server;
mod logging;
mod tls;
mod middleware;
mod error;

pub use server::ServerSettings;
pub use logging::{LogSettings, LogFormat, LogOutput};
pub use tls::TlsSettings;
pub use middleware::MiddlewareSettings;
pub use error::SettingsError;

use std::env;
use tracing::{warn, Level};

#[derive(Clone, Debug)]
pub struct Settings {
    /// 서버 설정
    pub server: ServerSettings,
    /// 로깅 설정
    #[serde(default)]
    pub logging: LogSettings,
    /// 미들웨어 설정
    #[serde(default)]
    pub middleware: MiddlewareSettings,
}

impl Settings {
    pub fn load() -> Result<Self, SettingsError> {
        if let Ok(config_path) = env::var("PROXY_CONFIG_FILE") {
            Self::from_toml_file(config_path)
        } else {
            Self::from_env()
        }
    }
} 