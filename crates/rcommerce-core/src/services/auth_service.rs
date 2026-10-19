use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use password_hash::rand_core::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest as Sha2Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

use crate::{Config, Error, Result};

/// Argon2id hasher instance
fn argon2() -> Argon2<'static> {
    Argon2::default()
}

#[derive(Clone)]
pub struct AuthService {
    config: Arc<Config>,
}

impl AuthService {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    /// Hash a password using Argon2id (modern standard)
    pub fn hash_password(&self, password: &str) -> Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        argon2()
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| Error::internal(format!("Failed to hash password: {}", e)))
    }

    /// Verify a password against a hash
    /// Supports Argon2id (native), bcrypt (legacy), and PHPass (WordPress/WooCommerce migrated)
    /// Returns (is_valid, needs_rehash) tuple
    pub fn verify_password(&self, password: &str, hash: &str) -> Result<(bool, bool)> {
        // Handle empty hash
        if hash.is_empty() {
            return Ok((false, false));
        }

        // Check if this is a PHPass hash (WordPress/WooCommerce)
        // PHPass hashes start with $P$ or $H$
        if hash.starts_with("$P$") || hash.starts_with("$H$") {
            let valid = self.verify_phpass(password, hash)?;
            return Ok((valid, valid)); // Needs rehash with Argon2id if valid
        }

        // Check if this is a bcrypt hash (legacy R Commerce)
        if hash.starts_with("$2a$") || hash.starts_with("$2b$") || hash.starts_with("$2y$") {
            let valid = self.verify_bcrypt(password, hash)?;
            return Ok((valid, valid)); // Needs rehash with Argon2id if valid
        }

        // Standard Argon2id verification
        let parsed_hash = match PasswordHash::new(hash) {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!("Invalid password hash format: {}", e);
                return Ok((false, false));
            }
        };

        let valid = argon2()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok();

        Ok((valid, false))
    }

    /// Verify bcrypt password (legacy support)
    fn verify_bcrypt(&self, password: &str, hash: &str) -> Result<bool> {
        use bcrypt::verify;
        verify(password, hash)
            .map_err(|e| Error::internal(format!("Failed to verify bcrypt password: {}", e)))
    }

    /// Verify PHPass password (WordPress/WooCommerce compatibility)
    /// PHPass uses MD5-based iterated hashing
    fn verify_phpass(&self, password: &str, hash: &str) -> Result<bool> {
        // PHPass hash format: $P$<iteration_count_char><salt><hash>
        // or $H$<iteration_count_char><salt><hash>
        if hash.len() < 12 {
            return Ok(false);
        }

        let itoa64 = "./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

        // Get iteration count character (at position 3)
        let iter_char = hash.chars().nth(3).unwrap_or('\0');
        let count_log2 = match itoa64.find(iter_char) {
            Some(pos) => pos,
            None => return Ok(false),
        };
        let count = 1usize << count_log2;

        // Get salt (8 characters starting at position 4)
        let salt = &hash[4..12.min(hash.len())];

        // Compute hash
        let mut computed_hash = format!("{}{}", salt, password);
        for _ in 0..count {
            computed_hash = format!("{:x}", md5::compute(computed_hash.as_bytes()));
        }

        // Encode to PHPass format
        let encoded = self.encode_phpass(&computed_hash);

        // Compare with stored hash (from position 12 onwards)
        let stored_hash_part = if hash.len() > 12 { &hash[12..] } else { "" };
        Ok(stored_hash_part == encoded)
    }

    /// Encode bytes to PHPass format
    fn encode_phpass(&self, input: &str) -> String {
        let itoa64 = "./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
        let input_bytes = hex::decode(input).unwrap_or_default();
        let mut output = String::new();

        let mut i = 0;
        while i < input_bytes.len() {
            let value = input_bytes[i] as usize;
            output.push(itoa64.chars().nth(value & 0x3f).unwrap());

            if i + 1 < input_bytes.len() {
                let value = ((input_bytes[i] as usize) << 8 | input_bytes[i + 1] as usize) >> 6;
                output.push(itoa64.chars().nth(value & 0x3f).unwrap());
            }

            i += 2;
        }

        output
    }

    /// Generate API key
    pub fn generate_api_key(&self) -> ApiKey {
        let prefix = self.generate_prefix();
        let secret = self.generate_secret();
        let full_key = format!("{}.{}", prefix, secret);

        let mut hasher = Sha256::new();
        hasher.update(full_key.as_bytes());
        let hash = hasher.finalize();
        let hash_hex = hex::encode(hash);

        ApiKey {
            prefix,
            full_key: Some(full_key),
            hash: hash_hex,
        }
    }

    /// Verify API key
    pub fn verify_api_key(&self, provided_key: &str, stored_hash: &str) -> Result<bool> {
        let parts: Vec<&str> = provided_key.splitn(2, '.').collect();
        if parts.len() != 2 {
            return Ok(false);
        }

        let mut hasher = Sha256::new();
        hasher.update(provided_key.as_bytes());
        let hash = hasher.finalize();
        let hash_hex = hex::encode(hash);

        Ok(hash_hex == stored_hash)
    }

    /// Generate JWT access token for a customer with role-based permissions
    pub fn generate_access_token(
        &self,
        customer_id: Uuid,
        email: &str,
        role: &crate::models::CustomerRole,
    ) -> Result<String> {
        let expiry_hours = self.config.security.jwt.expiry_hours as i64;
        let expiration = Utc::now()
            .checked_add_signed(Duration::hours(expiry_hours))
            .expect("valid timestamp")
            .timestamp();

        let claims = JwtClaims {
            sub: customer_id,
            email: email.to_string(),
            token_type: TokenType::Access,
            permissions: role.permissions(),
            exp: expiration,
            iat: Utc::now().timestamp(),
            iss: "rcommerce".to_string(),
            aud: "rcommerce-api".to_string(),
        };

        let header = Header::new(jsonwebtoken::Algorithm::HS256);
        let encoding_key = EncodingKey::from_secret(self.config.security.jwt.secret.as_bytes());

        encode(&header, &claims, &encoding_key)
            .map_err(|e| Error::internal(format!("Failed to generate JWT: {}", e)))
    }

    /// Generate JWT refresh token
    pub fn generate_refresh_token(&self, customer_id: Uuid, remember_me: bool) -> Result<String> {
        let expiry_hours = if remember_me {
            24 * 30 // 30 days
        } else {
            self.config.security.jwt.refresh_expiry_hours as i64
        };
        let expiration = Utc::now()
            .checked_add_signed(Duration::hours(expiry_hours))
            .expect("valid timestamp")
            .timestamp();

        let claims = JwtClaims {
            sub: customer_id,
            email: String::new(), // Refresh tokens don't need email
            token_type: TokenType::Refresh,
            permissions: vec!["refresh".to_string()],
            exp: expiration,
            iat: Utc::now().timestamp(),
            iss: "rcommerce".to_string(),
            aud: "rcommerce-api".to_string(),
        };

        let header = Header::new(jsonwebtoken::Algorithm::HS256);
        let encoding_key = EncodingKey::from_secret(self.config.security.jwt.secret.as_bytes());

        encode(&header, &claims, &encoding_key)
            .map_err(|e| Error::internal(format!("Failed to generate refresh token: {}", e)))
    }

    /// Verify and decode a JWT token
    pub fn verify_token(&self, token: &str) -> Result<JwtClaims> {
        let decoding_key = DecodingKey::from_secret(self.config.security.jwt.secret.as_bytes());
        let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
        // Disable audience validation since we're using simple validation
        validation.validate_aud = false;

        decode::<JwtClaims>(token, &decoding_key, &validation)
            .map(|data| data.claims)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
                    Error::unauthorized("Token has expired")
                }
                jsonwebtoken::errors::ErrorKind::InvalidToken => {
                    Error::unauthorized("Invalid token")
                }
                jsonwebtoken::errors::ErrorKind::InvalidSignature => {
                    Error::unauthorized("Invalid token signature")
                }
                _ => Error::unauthorized(format!("Token validation failed: {}", e)),
            })
    }

    /// Extract bearer token from Authorization header
    pub fn extract_bearer_token(auth_header: &str) -> Option<&str> {
        auth_header.strip_prefix("Bearer ")
    }

    /// Generate password reset token (short-lived)
    pub fn generate_password_reset_token(&self, customer_id: Uuid, email: &str) -> Result<String> {
        // Reset tokens last 1 hour
        let expiration = Utc::now()
            .checked_add_signed(Duration::hours(1))
            .expect("valid timestamp")
            .timestamp();

        let claims = JwtClaims {
            sub: customer_id,
            email: email.to_string(),
            token_type: TokenType::PasswordReset,
            permissions: vec!["password_reset".to_string()],
            exp: expiration,
            iat: Utc::now().timestamp(),
            iss: "rcommerce".to_string(),
            aud: "rcommerce-api".to_string(),
        };

        let header = Header::new(jsonwebtoken::Algorithm::HS256);
        let encoding_key = EncodingKey::from_secret(self.config.security.jwt.secret.as_bytes());

        encode(&header, &claims, &encoding_key)
            .map_err(|e| Error::internal(format!("Failed to generate password reset token: {}", e)))
    }

    /// Verify password reset token
    pub fn verify_password_reset_token(&self, token: &str) -> Result<JwtClaims> {
        let claims = self.verify_token(token)?;

        if claims.token_type != TokenType::PasswordReset {
            return Err(Error::unauthorized("Invalid token type"));
        }

        Ok(claims)
    }

    /// Generate email verification token (short-lived)
    pub fn generate_email_verification_token(
        &self,
        customer_id: Uuid,
        email: &str,
    ) -> Result<String> {
        let expiration = Utc::now()
            .checked_add_signed(Duration::hours(24))
            .expect("valid timestamp")
            .timestamp();

        let claims = JwtClaims {
            sub: customer_id,
            email: email.to_string(),
            token_type: TokenType::EmailVerification,
            permissions: vec!["email_verification".to_string()],
            exp: expiration,
            iat: Utc::now().timestamp(),
            iss: "rcommerce".to_string(),
            aud: "rcommerce-api".to_string(),
        };

        let header = Header::new(jsonwebtoken::Algorithm::HS256);
        let encoding_key = EncodingKey::from_secret(self.config.security.jwt.secret.as_bytes());

        encode(&header, &claims, &encoding_key).map_err(|e| {
            Error::internal(format!(
                "Failed to generate email verification token: {}",
                e
            ))
        })
    }

    /// Verify email verification token
    pub fn verify_email_verification_token(&self, token: &str) -> Result<JwtClaims> {
        let claims = self.verify_token(token)?;

        if claims.token_type != TokenType::EmailVerification {
            return Err(Error::unauthorized("Invalid token type"));
        }

        Ok(claims)
    }

    fn generate_prefix(&self) -> String {
        use rand::Rng;
        let prefix_length = self.config.security.api_key_prefix_length;

        rand::thread_rng()
            .sample_iter(&rand::distributions::Standard)
            .filter_map(|b: u8| {
                if b < 240 {
                    Some(format!("{:02x}", b))
                } else {
                    None
                }
            })
            .take(prefix_length)
            .collect::<String>()[..prefix_length]
            .to_string()
    }

    fn generate_secret(&self) -> String {
        use rand::Rng;
        let secret_length = self.config.security.api_key_secret_length;

        rand::thread_rng()
            .sample_iter(&rand::distributions::Standard)
            .filter_map(|b: u8| {
                if b < 240 {
                    Some(format!("{:02x}", b))
                } else {
                    None
                }
            })
            .take(secret_length)
            .collect::<String>()[..secret_length]
            .to_string()
    }
}

#[derive(Debug, Clone)]
pub struct ApiKey {
    pub prefix: String,
    pub full_key: Option<String>, // Only available on generation, never stored
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: Uuid, // User ID
    pub email: String,
    pub token_type: TokenType,
    pub permissions: Vec<String>,
    pub exp: i64,    // Expiration time
    pub iat: i64,    // Issued at
    pub iss: String, // Issuer
    pub aud: String, // Audience
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    Access,
    Refresh,
    PasswordReset,
    EmailVerification,
}

/// Authenticated user extracted from JWT
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub customer_id: Uuid,
    pub email: String,
    pub permissions: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        let mut config = Config::default();
        config.security.jwt.secret =
            "this_is_a_test_secret_that_is_at_least_32_bytes_long".to_string();
        config
    }

    #[test]
    fn test_generate_and_verify_api_key() {
        let config = test_config();
        let auth = AuthService::new(config);

        let api_key = auth.generate_api_key();
        assert!(api_key.full_key.is_some());

        let full_key = api_key.full_key.unwrap();
        let is_valid = auth.verify_api_key(&full_key, &api_key.hash).unwrap();
        assert!(is_valid);

        // Test invalid key
        let is_valid = auth.verify_api_key("invalid.key", &api_key.hash).unwrap();
        assert!(!is_valid);
    }

    #[test]
    fn test_password_hashing() {
        let config = test_config();
        let auth = AuthService::new(config);

        let password = "my_secure_password123";
        let hash = auth.hash_password(password).unwrap();

        // Verify correct password
        assert!(auth.verify_password(password, &hash).unwrap().0);

        // Verify wrong password fails
        assert!(!auth.verify_password("wrong_password", &hash).unwrap().0);
    }

    #[test]
    fn test_jwt_generation_and_verification() {
        use crate::models::CustomerRole;

        let config = test_config();
        let auth = AuthService::new(config);

        let customer_id = Uuid::new_v4();
        let email = "test@example.com";
        let role = CustomerRole::Customer;

        // Generate token
        let token = auth
            .generate_access_token(customer_id, email, &role)
            .unwrap();
        assert!(!token.is_empty());

        // Verify token
        let claims = auth.verify_token(&token).unwrap();
        assert_eq!(claims.sub, customer_id);
        assert_eq!(claims.email, email);
        assert_eq!(claims.token_type, TokenType::Access);
        // Customer role should only have "read" permission
        assert!(claims.permissions.contains(&"read".to_string()));
        assert!(!claims.permissions.contains(&"write".to_string()));
    }

    #[test]
    fn test_jwt_permissions_by_role() {
        use crate::models::CustomerRole;

        let config = test_config();
        let auth = AuthService::new(config);

        let customer_id = Uuid::new_v4();
        let email = "test@example.com";

        // Test Customer role
        let token = auth
            .generate_access_token(customer_id, email, &CustomerRole::Customer)
            .unwrap();
        let claims = auth.verify_token(&token).unwrap();
        assert_eq!(claims.permissions, vec!["read"]);

        // Test Manager role
        let token = auth
            .generate_access_token(customer_id, email, &CustomerRole::Manager)
            .unwrap();
        let claims = auth.verify_token(&token).unwrap();
        assert!(claims.permissions.contains(&"read".to_string()));
        assert!(claims.permissions.contains(&"write".to_string()));

        // Test Admin role
        let token = auth
            .generate_access_token(customer_id, email, &CustomerRole::Admin)
            .unwrap();
        let claims = auth.verify_token(&token).unwrap();
        assert!(claims.permissions.contains(&"read".to_string()));
        assert!(claims.permissions.contains(&"write".to_string()));
        assert!(claims.permissions.contains(&"admin".to_string()));
    }

    #[test]
    fn test_extract_bearer_token() {
        assert_eq!(
            AuthService::extract_bearer_token("Bearer my_token_123"),
            Some("my_token_123")
        );
        assert_eq!(
            AuthService::extract_bearer_token("Basic dXNlcjpwYXNz"),
            None
        );
        assert_eq!(AuthService::extract_bearer_token("my_token_123"), None);
    }
}
