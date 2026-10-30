use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use once_cell::sync::Lazy;
use rand::RngCore;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};

/// 32 random bytes, hex encoded (64 chars). Used for sessions, invites, reset links.
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex(&bytes)
}

pub fn sha256(data: &str) -> Vec<u8> {
    Sha256::digest(data.as_bytes()).to_vec()
}

pub fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

/// Constant-time comparison, for flags and other secrets compared in memory.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Argon2id is CPU heavy (~tens of ms): run it off the async workers.
pub async fn hash_password(password: String) -> String {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .expect("argon2 hash")
            .to_string()
    })
    .await
    .expect("hash task")
}

/// Hash compared against when the user does not exist, so timing does not reveal valid usernames.
static DUMMY_HASH: Lazy<String> = Lazy::new(|| {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(b"not-a-real-password", &salt)
        .unwrap()
        .to_string()
});

pub async fn verify_password(password: String, hash: Option<String>) -> bool {
    tokio::task::spawn_blocking(move || {
        let found = hash.is_some();
        let hash = hash.unwrap_or_else(|| DUMMY_HASH.clone());
        let Ok(parsed) = PasswordHash::new(&hash) else {
            return false;
        };
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
            && found
    })
    .await
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_ct_eq() {
        assert_eq!(hex(&[0x00, 0xab, 0xff]), "00abff");
        assert!(ct_eq(b"LH{a}", b"LH{a}"));
        assert!(!ct_eq(b"LH{a}", b"LH{b}"));
        assert!(!ct_eq(b"LH{a}", b"LH{ab}"));
        assert_eq!(random_token().len(), 64);
    }

    #[tokio::test]
    async fn password_roundtrip() {
        let h = hash_password("correct horse".into()).await;
        assert!(verify_password("correct horse".into(), Some(h.clone())).await);
        assert!(!verify_password("wrong".into(), Some(h)).await);
        assert!(!verify_password("x".into(), None).await);
    }
}
