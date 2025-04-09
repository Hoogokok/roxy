use std::collections::HashMap;
use std::marker::PhantomData;
use serde::Deserialize;
use serde::de::Error;

use crate::settings::core::{AnyLoadBalancerSettings, Settings};
use crate::settings::server::HttpsDisabled;
use crate::settings::typestate::{Raw, Validated};
use crate::settings::logging::LogSettings;
use crate::settings::tls::TlsSettings;
use crate::settings::docker::DockerSettings;
use crate::middleware::config::MiddlewareConfig;
use crate::settings::types::ValidMiddlewareId;
use crate::settings::load_balancer::{LoadBalancerSettings, NoLoadBalancing};

/// Settings 역직렬화 구현
impl<'de> Deserialize<'de> for Settings<Validated, HttpsDisabled> {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        /// 내부 헬퍼 구조체
        #[derive(Deserialize)]
        struct SettingsHelper {
            #[serde(default)]
            logging: LogSettings<Raw>,
            #[serde(default)]
            tls: TlsSettings<Raw>,
            #[serde(default)]
            docker: DockerSettings<Raw>,
            #[serde(default)]
            middleware: HashMap<String, MiddlewareConfig>,
            #[serde(default)]
            router_middlewares: HashMap<String, Vec<ValidMiddlewareId>>,
        }
        
        let settings_helper = SettingsHelper::deserialize(deserializer)?;
        
        // Raw 상태의 로깅 설정을 Validated 상태로 변환
        let validated_logging = settings_helper.logging.validated()
            .map_err(D::Error::custom)?;
            
        // Raw 상태의 TLS 설정을 Validated 상태로 변환
        let validated_tls = tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(settings_helper.tls.validated())
        }).map_err(D::Error::custom)?;
        
        // Raw 상태의 Docker 설정을 Validated 상태로 변환
        let validated_docker = settings_helper.docker.validated()
            .map_err(D::Error::custom)?;
            
        Ok(Settings {
            server: crate::settings::server::ServerSettings::<Validated, HttpsDisabled>::default(),
            logging: validated_logging,
            tls: validated_tls,
            docker: validated_docker,
            middleware: settings_helper.middleware,
            router_middlewares: settings_helper.router_middlewares,
            load_balancer: AnyLoadBalancerSettings::None(LoadBalancerSettings::<Validated, NoLoadBalancing>::new()),
            _marker: PhantomData,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[test]
    fn test_deserialize_settings() {
        let json_value = json!({
            "logging": {
                "format": "text",
                "level": "info"
            },
            "middleware": {
                "cors": {
                    "type": "cors",
                    "enabled": true,
                    "order": 0,
                    "settings": {
                        "cors.allowOrigins": "*"
                    }
                }
            }
        });
        
        let settings: Result<Settings<Validated, HttpsDisabled>, _> = serde_json::from_value(json_value);
        assert!(settings.is_ok());
        
        let settings = settings.unwrap();
        assert_eq!(settings.middleware.len(), 1);
        assert!(settings.middleware.contains_key("cors"));
    }
} 