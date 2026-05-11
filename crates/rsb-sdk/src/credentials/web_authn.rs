pub struct WebAuthn {
    pub authenticator: Authenticator,
    pub credential: Credential,
}

impl WebAuthn {
    pub fn new() -> Self {
        Self {
            authenticator: Authenticator::new(),
            credential: Credential::new(),
        }
    }
}
impl Default for WebAuthn {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Debug)]
pub struct Authenticator {
    pub id: String,
    pub public_key: Vec<u8>,
}

impl Authenticator {
    pub fn new() -> Self {
        Self {
            id: "authenticator-id".to_string(),
            public_key: vec![0; 32], // Placeholder for a real public key
        }
    }
}
impl Default for Authenticator {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct Credential {
    pub id: String,
    pub public_key: Vec<u8>,
}

impl Default for Credential {
    fn default() -> Self {
        Self::new()
    }
}
impl Credential {
    pub fn new() -> Self {
        Self {
            id: "credential-id".to_string(),
            public_key: vec![0; 32], // Placeholder for a real public key
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webauthn_new() {
        let webauthn = WebAuthn::new();
        assert_eq!(webauthn.authenticator.id, "authenticator-id");
        assert_eq!(webauthn.credential.id, "credential-id");
    }
}
