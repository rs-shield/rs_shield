/// SecureString: Sensitive data type that automatically zeros memory
/// Implements Zeroize to clear confidential data from memory
use serde::{Deserialize, Serialize};
use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, Zeroize, ZeroizeOnDrop, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecureString {
    #[serde(flatten)]
    value: String,
}

impl SecureString {
    /// Create from a String
    pub fn new(value: String) -> Self {
        SecureString { value }
    }

    /// Get immutable reference
    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// Consume and return original String (not possible with ZeroizeOnDrop)
    pub fn as_inner(&self) -> &str {
        &self.value
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    /// Validate if in valid format
    pub fn is_valid(&self) -> bool {
        !self.value.is_empty() && self.value.len() >= 16
    }
}

/// Implement safe Debug
impl fmt::Debug for SecureString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Don't display content in debug, just indicate it's sensitive
        f.debug_struct("SecureString")
            .field("value", &"***REDACTED***")
            .finish()
    }
}

/// Implementar Display seguro (também oculta)
impl fmt::Display for SecureString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "***REDACTED***")
    }
}

/// Convertê de &str
impl From<&str> for SecureString {
    fn from(s: &str) -> Self {
        SecureString::new(s.to_string())
    }
}

/// Comparação segura (resistente a timing attacks por usar constante-time)
impl PartialEq for SecureString {
    fn eq(&self, other: &Self) -> bool {
        // Usar comparação timing-safe se possível
        self.value.len() == other.value.len()
            && self
                .value
                .as_bytes()
                .iter()
                .zip(other.value.as_bytes())
                .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                == 0
    }
}

impl Eq for SecureString {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secure_string_not_displayed() {
        let secret = SecureString::new("my-secret-key".to_string());
        let display = format!("{}", secret);
        assert_eq!(display, "***REDACTED***");
    }

    #[test]
    fn test_secure_string_debug_not_shown() {
        let secret = SecureString::new("my-secret-key".to_string());
        let debug = format!("{:?}", secret);
        assert!(debug.contains("REDACTED"));
        assert!(!debug.contains("my-secret"));
    }

    #[test]
    fn test_secure_string_equality() {
        let secret1 = SecureString::new("same-secret".to_string());
        let secret2 = SecureString::new("same-secret".to_string());
        let secret3 = SecureString::new("different".to_string());

        assert_eq!(secret1, secret2);
        assert_ne!(secret1, secret3);
    }
}
