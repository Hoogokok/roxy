//! Strong types for configuration validation
//! 
//! This module contains type definitions that enforce compile-time validation
//! according to the "Parse, don't validate" principle.

use std::fmt;

/// A validated service identifier.
/// 
/// This type guarantees that the contained ID is valid according to our service
/// naming rules. Once a `ValidServiceId` is created, it is guaranteed to be valid
/// for the lifetime of the program.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ValidServiceId(String);

impl ValidServiceId {
    /// Attempts to create a new `ValidServiceId` from a string.
    /// 
    /// Returns `None` if the service ID is invalid.
    pub fn new(id: impl Into<String>) -> Option<Self> {
        let id = id.into();
        
        // Service ID validation rules:
        // - Cannot be empty
        // - Must only contain alphanumeric characters, dashes, or underscores
        if id.is_empty() || !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return None;
        }
        
        Some(ValidServiceId(id))
    }
    
    /// Returns the inner string value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
    
    /// Unwraps the `ValidServiceId` into its inner String.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for ValidServiceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A validated middleware identifier.
/// 
/// This type guarantees that the contained ID is valid according to our middleware
/// naming rules. Once a `ValidMiddlewareId` is created, it is guaranteed to be valid
/// for the lifetime of the program.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ValidMiddlewareId(String);

impl ValidMiddlewareId {
    /// Attempts to create a new `ValidMiddlewareId` from a string.
    /// 
    /// Returns `None` if the middleware ID is invalid.
    pub fn new(id: impl Into<String>) -> Option<Self> {
        let id = id.into();
        
        // Middleware ID validation rules:
        // - Cannot be empty
        // - Must only contain alphanumeric characters, dashes, or underscores
        if id.is_empty() || !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return None;
        }
        
        Some(ValidMiddlewareId(id))
    }
    
    /// Returns the inner string value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
    
    /// Unwraps the `ValidMiddlewareId` into its inner String.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for ValidMiddlewareId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A validated router identifier.
/// 
/// This type guarantees that the contained ID is valid according to our router
/// naming rules. Once a `ValidRouterId` is created, it is guaranteed to be valid
/// for the lifetime of the program.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ValidRouterId(String);

impl ValidRouterId {
    /// Attempts to create a new `ValidRouterId` from a string.
    /// 
    /// Returns `None` if the router ID is invalid.
    pub fn new(id: impl Into<String>) -> Option<Self> {
        let id = id.into();
        
        // Router ID validation rules:
        // - Cannot be empty
        // - Must only contain alphanumeric characters, dashes, or underscores
        if id.is_empty() || !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return None;
        }
        
        Some(ValidRouterId(id))
    }
    
    /// Returns the inner string value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
    
    /// Unwraps the `ValidRouterId` into its inner String.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for ValidRouterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_service_ids() {
        // Valid service IDs
        let valid_ids = vec![
            "service1",
            "service-name",
            "service_name",
            "SERVICE_NAME",
            "123service",
            "s",
        ];

        for id in valid_ids {
            let valid_id = ValidServiceId::new(id);
            assert!(valid_id.is_some(), "Expected '{}' to be a valid service ID", id);
            
            if let Some(valid_id) = valid_id {
                // Test as_str() returns the original string
                assert_eq!(valid_id.as_str(), id);
                
                // Test into_inner() returns the original string
                assert_eq!(valid_id.into_inner(), id);
            }
        }
    }

    #[test]
    fn test_invalid_service_ids() {
        // Invalid service IDs
        let invalid_ids = vec![
            "", // Empty string
            "service name", // Contains spaces
            "service.name", // Contains periods
            "service/name", // Contains slashes
            "service:name", // Contains colons
            "service!",     // Contains special characters
        ];

        for id in invalid_ids {
            let invalid_id = ValidServiceId::new(id);
            assert!(invalid_id.is_none(), "Expected '{}' to be an invalid service ID", id);
        }
    }

    #[test]
    fn test_valid_middleware_ids() {
        // Valid middleware IDs
        let valid_ids = vec![
            "cors",
            "api-cors",
            "auth_middleware",
            "RATE_LIMIT",
            "123middleware",
            "m",
        ];

        for id in valid_ids {
            let valid_id = ValidMiddlewareId::new(id);
            assert!(valid_id.is_some(), "Expected '{}' to be a valid middleware ID", id);
            
            if let Some(valid_id) = valid_id {
                // Test as_str() returns the original string
                assert_eq!(valid_id.as_str(), id);
                
                // Test into_inner() returns the original string
                assert_eq!(valid_id.into_inner(), id);
            }
        }
    }

    #[test]
    fn test_invalid_middleware_ids() {
        // Invalid middleware IDs
        let invalid_ids = vec![
            "", // Empty string
            "middleware name", // Contains spaces
            "middleware.name", // Contains periods
            "middleware/name", // Contains slashes
            "middleware:name", // Contains colons
            "middleware!",     // Contains special characters
        ];

        for id in invalid_ids {
            let invalid_id = ValidMiddlewareId::new(id);
            assert!(invalid_id.is_none(), "Expected '{}' to be an invalid middleware ID", id);
        }
    }

    #[test]
    fn test_valid_router_ids() {
        // Valid router IDs
        let valid_ids = vec![
            "api",
            "web-router",
            "auth_router",
            "MAIN_ROUTER",
            "123router",
            "r",
        ];

        for id in valid_ids {
            let valid_id = ValidRouterId::new(id);
            assert!(valid_id.is_some(), "Expected '{}' to be a valid router ID", id);
            
            if let Some(valid_id) = valid_id {
                // Test as_str() returns the original string
                assert_eq!(valid_id.as_str(), id);
                
                // Test into_inner() returns the original string
                assert_eq!(valid_id.into_inner(), id);
            }
        }
    }

    #[test]
    fn test_invalid_router_ids() {
        // Invalid router IDs
        let invalid_ids = vec![
            "", // Empty string
            "router name", // Contains spaces
            "router.name", // Contains periods
            "router/name", // Contains slashes
            "router:name", // Contains colons
            "router!",     // Contains special characters
        ];

        for id in invalid_ids {
            let invalid_id = ValidRouterId::new(id);
            assert!(invalid_id.is_none(), "Expected '{}' to be an invalid router ID", id);
        }
    }

    #[test]
    fn test_display_implementation() {
        // Test display for ValidServiceId
        let service_id = "test-service";
        let valid_service_id = ValidServiceId::new(service_id).unwrap();
        assert_eq!(format!("{}", valid_service_id), service_id);
        
        // Test display for ValidMiddlewareId
        let middleware_id = "test-middleware";
        let valid_middleware_id = ValidMiddlewareId::new(middleware_id).unwrap();
        assert_eq!(format!("{}", valid_middleware_id), middleware_id);
        
        // Test display for ValidRouterId
        let router_id = "test-router";
        let valid_router_id = ValidRouterId::new(router_id).unwrap();
        assert_eq!(format!("{}", valid_router_id), router_id);
    }
} 