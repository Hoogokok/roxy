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