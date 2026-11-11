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
    fn test_student_anonymization() {
        let anon = anonymize_student_id("stu_001", "lagos_jss_salt");
        assert!(anon.starts_with("anon_"));
        assert_eq!(anon.len(), 21);

        let moniker = anonymized_display_moniker(&anon);
        assert!(moniker.starts_with("Scholar #"));
    }
}
