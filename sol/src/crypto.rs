use ed25519_dalek::{Signer, Verifier, SigningKey, VerifyingKey, Signature as DalekSignature};
use sha2::{Digest, Sha512};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

#[derive(Clone)]
pub struct Keypair {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

impl Keypair {
    pub fn generate() -> Self {
        let mut seed = [0u8; 32];
        use rand::RngCore;
        rand::rngs::OsRng.fill_bytes(&mut seed);
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();
        Self { signing_key, verifying_key }
    }

    pub fn from_base64(seed_b64: &str) -> Result<Self, String> {
        let bytes = BASE64.decode(seed_b64)
            .map_err(|e| format!("invalid key seed: {}", e))?;
        if bytes.len() != 32 {
            return Err(format!("key seed must be 32 bytes, got {}", bytes.len()));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        let signing_key = SigningKey::from_bytes(&arr);
        let verifying_key = signing_key.verifying_key();
        Ok(Self { signing_key, verifying_key })
    }

    pub fn to_base64_seed(&self) -> String {
        BASE64.encode(self.signing_key.to_bytes())
    }

    pub fn public_key_base64(&self) -> String {
        BASE64.encode(self.verifying_key.to_bytes())
    }

    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }

    pub fn sign(&self, msg: &[u8]) -> Vec<u8> {
        let sig: DalekSignature = self.signing_key.sign(msg);
        sig.to_bytes().to_vec()
    }
}

pub fn verify(pk_base64: &str, msg: &[u8], sig_bytes: &[u8]) -> Result<(), String> {
    let pk_bytes = BASE64.decode(pk_base64)
        .map_err(|e| format!("invalid public key: {}", e))?;
    if pk_bytes.len() != 32 {
        return Err("public key must be 32 bytes".into());
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&pk_bytes);
    let verifying_key = VerifyingKey::from_bytes(&arr)
        .map_err(|e| format!("invalid public key: {}", e))?;

    let mut sig_arr = [0u8; 64];
    if sig_bytes.len() != 64 {
        return Err("signature must be 64 bytes".into());
    }
    sig_arr.copy_from_slice(sig_bytes);
    let sig = DalekSignature::from_bytes(&sig_arr);

    verifying_key.verify(msg, &sig)
        .map_err(|e| format!("signature verification failed: {}", e))
}

pub fn sha512_digest(body: &[u8]) -> String {
    let mut hasher = Sha512::new();
    hasher.update(body);
    BASE64.encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_verify() {
        let alice = Keypair::generate();
        let bob = Keypair::generate();

        let msg = b"hello world";
        let sig = alice.sign(msg);

        // Alice's sig verifies with Alice's pk
        assert!(verify(&alice.public_key_base64(), msg, &sig).is_ok());

        // Alice's sig fails with Bob's pk
        assert!(verify(&bob.public_key_base64(), msg, &sig).is_err());

        // Modified message fails
        assert!(verify(&alice.public_key_base64(), b"HELLO WORLD", &sig).is_err());
    }

    #[test]
    fn test_from_base64_roundtrip() {
        let kp = Keypair::generate();
        let seed = kp.to_base64_seed();
        let pk = kp.public_key_base64();

        let restored = Keypair::from_base64(&seed).unwrap();
        assert_eq!(restored.public_key_base64(), pk);

        // Restored keypair signs and verifies
        let msg = b"test message";
        let sig = restored.sign(msg);
        assert!(verify(&pk, msg, &sig).is_ok());
    }

    #[test]
    fn test_invalid_seed() {
        assert!(Keypair::from_base64("too-short").is_err());
        assert!(Keypair::from_base64("!!!invalid-base64!!!").is_err());
    }

    #[test]
    fn test_sha512_digest() {
        let d = sha512_digest(b"hello");
        assert!(!d.is_empty());
        // deterministic
        assert_eq!(sha512_digest(b"hello"), sha512_digest(b"hello"));
        assert_ne!(sha512_digest(b"hello"), sha512_digest(b"world"));
    }
}
