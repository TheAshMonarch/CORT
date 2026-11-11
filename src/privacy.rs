use axum::http::HeaderMap;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

/// Validates a lightweight offline PIN (4 to 8 numeric digits).
pub fn validate_pin_format(pin: &str) -> AppResult<()> {
    if pin.len() < 4 || pin.len() > 8 || !pin.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::BadRequest(
            "PIN must be between 4 and 8 numeric digits".to_string(),
        ));
    }
    Ok(())
}

/// Computes a salted SHA-256 hash of a student's PIN for local storage.
pub fn hash_pin(pin: &str, school_salt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(school_salt.as_bytes());
    hasher.update(b":pin:");
    hasher.update(pin.as_bytes());
    hex::encode(hasher.finalize())
}

/// Computes a salted SHA-256 hash of the teacher/operator's admin PIN.
pub fn hash_teacher_pin(pin: &str, school_salt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(school_salt.as_bytes());
    hasher.update(b":teacher_pin:");
    hasher.update(pin.as_bytes());
    hex::encode(hasher.finalize())
}

/// Issues a signed session token for an authenticated student on the local Host Device.
pub fn issue_student_token(student_id: &str, school_salt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(school_salt.as_bytes());
    hasher.update(b":student_session:");
    hasher.update(student_id.as_bytes());
    let digest = hex::encode(hasher.finalize());
    format!("{student_id}.{}", &digest[..24])
}

/// Verifies a student's session token against the expected `student_id`.
pub fn verify_student_token(
    token: &str,
    expected_student_id: &str,
    school_salt: &str,
) -> AppResult<()> {
    let expected = issue_student_token(expected_student_id, school_salt);
    if token.trim() != expected {
        return Err(AppError::Unauthorized(
            "Invalid or expired student session token".to_string(),
        ));
    }
    Ok(())
}

/// Extracts a Bearer token from the HTTP `Authorization` header if present.
pub fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer ").or_else(|| s.strip_prefix("bearer ")))
        .map(str::trim)
}

/// Extracts the Teacher PIN from the `x-teacher-pin` HTTP header if present.
pub fn extract_teacher_pin(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("x-teacher-pin")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
}

/// Generates an irreversible, deterministic anonymized student ID for Cloud Sync.
/// Ensures student PII (real name/nickname) never leaves the Host Device.
pub fn anonymize_student_id(student_id: &str, school_salt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(school_salt.as_bytes());
    hasher.update(b":student:");
    hasher.update(student_id.as_bytes());
    let digest = hex::encode(hasher.finalize());
    format!("anon_{}", &digest[..16])
}

/// Generates a privacy-preserving public display alias from an anonymized ID
/// (e.g., "Scholar #4F9A") for national leaderboards.
pub fn anonymized_display_moniker(anon_id: &str) -> String {
    let suffix = anon_id
        .strip_prefix("anon_")
        .unwrap_or(anon_id)
        .chars()
        .take(4)
        .collect::<String>()
        .to_uppercase();
    format!("Scholar #{}", suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pin_validation_and_hashing() {
        assert!(validate_pin_format("1234").is_ok());
        assert!(validate_pin_format("123456").is_ok());
        assert!(validate_pin_format("12").is_err());
        assert!(validate_pin_format("abcd").is_err());

        let h1 = hash_pin("1234", "salt_a");
        let h2 = hash_pin("1234", "salt_a");
        let h3 = hash_pin("1234", "salt_b");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_student_session_token() {
        let token = issue_student_token("stu_abc123", "salt_xyz");
        assert!(verify_student_token(&token, "stu_abc123", "salt_xyz").is_ok());
        assert!(verify_student_token(&token, "stu_other", "salt_xyz").is_err());
        assert!(verify_student_token("stu_abc123.forged", "stu_abc123", "salt_xyz").is_err());
    }

    #[test]
    fn test_student_anonymization() {
        let anon = anonymize_student_id("stu_001", "lagos_jss_salt");
        assert!(anon.starts_with("anon_"));
        assert_eq!(anon.len(), 21);

        let moniker = anonymized_display_moniker(&anon);
        assert!(moniker.starts_with("Scholar #"));
    }
}
