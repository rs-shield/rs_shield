use std::fmt::{Display, Formatter};

#[derive(Debug, Clone)]
pub enum Fido2Error {
    NotFound,
    AlreadyExists,
    Registration(String),
    Authentication(String),
    InvalidState,
    StorageError(String),
    ValidationError(String),
}

impl Display for Fido2Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "Credential not found"),
            Self::AlreadyExists => write!(f, "Credential already exists for this user"),
            Self::Registration(e) => write!(f, "Registration error: {}", e),
            Self::Authentication(e) => write!(f, "Authentication error: {}", e),
            Self::InvalidState => write!(f, "Invalid or expired state"),
            Self::StorageError(e) => write!(f, "Storage error: {}", e),
            Self::ValidationError(e) => write!(f, "Validation error: {}", e),
        }
    }
}

impl std::error::Error for Fido2Error {}

impl From<Fido2Error> for String {
    fn from(err: Fido2Error) -> Self {
        err.to_string()
    }
}
