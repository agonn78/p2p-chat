//! End-to-End Encryption module using X25519 key exchange and AES-256-GCM
//!
//! Flow:
//! 1. Generate ephemeral X25519 keypair
//! 2. Exchange public keys via signaling server
//! 3. Derive shared secret using Diffie-Hellman
//! 4. Use shared secret as AES-256-GCM key for encrypting audio packets

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey, NONCE_LEN};
use ring::agreement::{self, EphemeralPrivateKey, UnparsedPublicKey, X25519};
use ring::rand::SystemRandom;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Cryptographic context for E2EE communication
/// Thread-safe wrapper around ring's AES-GCM key
pub struct CryptoContext {
    /// AES-GCM key derived from X25519 key exchange, wrapped in Mutex for thread-safety
    key: Mutex<LessSafeKey>,
    /// Counter for generating unique nonces
    nonce_counter: AtomicU64,
}

// Explicitly implement Send + Sync since we're protecting access with Mutex
unsafe impl Send for CryptoContext {}
unsafe impl Sync for CryptoContext {}

/// Key pair for X25519 key exchange
pub struct KeyPair {
    private_key: EphemeralPrivateKey,
    pub public_key_bytes: Vec<u8>,
}

impl KeyPair {
    /// Generate a new X25519 key pair
    pub fn generate() -> Result<Self, ring::error::Unspecified> {
        let rng = SystemRandom::new();
        let private_key = EphemeralPrivateKey::generate(&X25519, &rng)?;
        let public_key = private_key.compute_public_key()?;

        Ok(Self {
            private_key,
            public_key_bytes: public_key.as_ref().to_vec(),
        })
    }

    /// Get public key as base64 string for transmission
    pub fn public_key_base64(&self) -> String {
        BASE64.encode(&self.public_key_bytes)
    }

    /// Perform key exchange with peer's public key and derive encryption context
    pub fn derive_shared_secret(
        self,
        peer_public_key_bytes: &[u8],
    ) -> Result<CryptoContext, String> {
        let peer_public_key = UnparsedPublicKey::new(&X25519, peer_public_key_bytes);

        let shared_secret =
            agreement::agree_ephemeral(self.private_key, &peer_public_key, |key_material| {
                // Use first 32 bytes as AES-256 key
                let mut key_bytes = [0u8; 32];
                key_bytes.copy_from_slice(&key_material[..32]);
                key_bytes
            })
            .map_err(|_| "Key exchange failed".to_string())?;

        CryptoContext::new(&shared_secret)
    }
}

impl CryptoContext {
    /// Create a new crypto context from a 32-byte key
    fn new(key_bytes: &[u8; 32]) -> Result<Self, String> {
        let unbound_key = UnboundKey::new(&aead::AES_256_GCM, key_bytes)
            .map_err(|_| "Failed to create AES key".to_string())?;

        Ok(Self {
            key: Mutex::new(LessSafeKey::new(unbound_key)),
            nonce_counter: AtomicU64::new(0),
        })
    }

    /// Generate a unique nonce for encryption
    fn next_nonce(&self) -> [u8; NONCE_LEN] {
        let counter = self.nonce_counter.fetch_add(1, Ordering::SeqCst);
        let mut nonce = [0u8; NONCE_LEN];
        // Put counter in the last 8 bytes of the nonce
        nonce[4..12].copy_from_slice(&counter.to_be_bytes());
        nonce
    }

    /// Encrypt audio data in-place
    /// Returns the nonce prepended to the ciphertext
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        let nonce_bytes = self.next_nonce();
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);

        let mut buffer = plaintext.to_vec();
        // Reserve space for the authentication tag
        buffer.extend_from_slice(&[0u8; 16]);

        let key = self.key.lock().map_err(|_| "Lock poisoned")?;
        key.seal_in_place_separate_tag(nonce, Aad::empty(), &mut buffer[..plaintext.len()])
            .map(|tag| {
                buffer[plaintext.len()..].copy_from_slice(tag.as_ref());
                let mut result = nonce_bytes.to_vec();
                result.extend_from_slice(&buffer);
                result
            })
            .map_err(|_| "Encryption failed".to_string())
    }

    /// Decrypt audio data
    /// Expects nonce prepended to ciphertext
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        if ciphertext.len() < NONCE_LEN + 16 {
            return Err("Ciphertext too short".to_string());
        }

        let (nonce_bytes, encrypted) = ciphertext.split_at(NONCE_LEN);
        let nonce = Nonce::try_assume_unique_for_key(nonce_bytes)
            .map_err(|_| "Invalid nonce".to_string())?;

        let mut buffer = encrypted.to_vec();

        let key = self.key.lock().map_err(|_| "Lock poisoned")?;
        key.open_in_place(nonce, Aad::empty(), &mut buffer)
            .map(|plaintext| plaintext.to_vec())
            .map_err(|_| "Decryption failed".to_string())
    }
}

/// Parse a base64 encoded public key
pub fn parse_public_key(base64_key: &str) -> Result<Vec<u8>, String> {
    BASE64
        .decode(base64_key)
        .map_err(|e| format!("Invalid base64: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_exchange_and_encryption() {
        // Simulate two parties
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        // Exchange public keys (normally via signaling server)
        let alice_public = alice.public_key_bytes.clone();
        let bob_public = bob.public_key_bytes.clone();

        // Derive shared secrets
        let alice_ctx = alice.derive_shared_secret(&bob_public).unwrap();
        let bob_ctx = bob.derive_shared_secret(&alice_public).unwrap();

        // Test encryption from Alice to Bob
        let plaintext = b"Hello, encrypted world!";
        let ciphertext = alice_ctx.encrypt(plaintext).unwrap();
        let decrypted = bob_ctx.decrypt(&ciphertext).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }
}
