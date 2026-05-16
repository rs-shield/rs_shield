use crate::auth::types::SessionClaims;
use chrono::Utc;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use uuid::Uuid;

pub struct JwtManager {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
}

impl JwtManager {
    pub fn new(secret: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let secret_bytes = secret.as_bytes();

        Ok(Self {
            encoding_key: EncodingKey::from_secret(secret_bytes),
            decoding_key: DecodingKey::from_secret(secret_bytes),
        })
    }

    pub fn create_token(
        &self,
        user_id: &str,
        scopes: Vec<String>,
        expires_in_seconds: i64,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let now = Utc::now().timestamp();
        let jti = Uuid::new_v4().to_string();

        let claims = SessionClaims {
            sub: user_id.to_string(),
            scopes,
            jti,
            iat: now,
            exp: now + expires_in_seconds,
            aud: "rsb-shield".to_string(),
        };

        let token = encode(&Header::default(), &claims, &self.encoding_key)?;
        Ok(token)
    }

    pub fn verify_token(&self, token: &str) -> Result<SessionClaims, Box<dyn std::error::Error>> {
        let mut validation = Validation::default();
        validation.set_audience(&["rsb-shield"]);

        let token_data = decode::<SessionClaims>(token, &self.decoding_key, &validation)?;

        Ok(token_data.claims)
    }

    pub fn is_expired(&self, token: &str) -> bool {
        match self.verify_token(token) {
            Ok(claims) => {
                let now = Utc::now().timestamp();
                claims.exp < now
            }
            Err(_) => true,
        }
    }

    pub fn extract_jti(&self, token: &str) -> Option<String> {
        match self.verify_token(token) {
            Ok(claims) => Some(claims.jti),
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jwt_create_and_verify() {
        let manager = JwtManager::new("test-secret").unwrap();
        let token = manager
            .create_token("user1", vec!["backup".to_string()], 2592000)
            .unwrap();

        let claims = manager.verify_token(&token).unwrap();
        assert_eq!(claims.sub, "user1");
        assert_eq!(claims.scopes, vec!["backup".to_string()]);
        assert!(!claims.jti.is_empty());
    }

    #[test]
    fn test_jwt_invalid_signature() {
        let manager1 = JwtManager::new("secret1").unwrap();
        let manager2 = JwtManager::new("secret2").unwrap();

        let token = manager1
            .create_token("user1", vec!["backup".to_string()], 3600)
            .unwrap();

        let result = manager2.verify_token(&token);
        assert!(result.is_err());
    }
}
