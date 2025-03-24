#[cfg(test)]
mod config_path_tests {
    use crate::settings::{types::ConfigPath, typestate::{Raw, Validatable}};

    use super::*;
    
    #[test]
    fn test_new_config_path_raw() {
        let path = ConfigPath::<Raw>::new("/app/config.json".to_string());
        assert_eq!(path.as_str(), "/app/config.json");
    }
    
    #[test]
    fn test_validate_valid_path() {
        let raw_path = ConfigPath::<Raw>::new("/app/config.json".to_string());
        let validated = raw_path.validate();
        assert!(validated.is_ok());
        assert_eq!(validated.unwrap().as_str(), "/app/config.json");
    }
    
    #[test]
    fn test_validate_empty_path() {
        let raw_path = ConfigPath::<Raw>::new("".to_string());
        let validated = raw_path.validate();
        assert!(validated.is_err());
    }
    
    #[test]
    fn test_validate_relative_path() {
        let raw_path = ConfigPath::<Raw>::new("config.json".to_string());
        let validated = raw_path.validate();
        assert!(validated.is_err());
    }
    
    #[test]
    fn test_validated_path_cannot_be_created_directly() {
        // 컴파일 에러가 발생해야 합니다 - 타입 시스템이 작동한다는 증거
        // 이 테스트는 컴파일되지 않아야 하며, 주석 처리되어야 합니다
        // let path = ConfigPath::<Validated>::new("/app/config.json".to_string()); // 컴파일 에러!
    }
}
