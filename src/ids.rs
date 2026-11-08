use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};

const B58: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// Random base58 string of `len` chars (uniform, no modulo bias worth caring about at 58/256).
pub fn random_base58(len: usize) -> String {
    let mut out = String::with_capacity(len);
    while out.len() < len {
        let mut buf = [0u8; 32];
        rand::fill(&mut buf);
        for b in buf {
            // reject the tail to stay uniform: 58*4 = 232
            if b < 232 {
                out.push(B58[(b % 58) as usize] as char);
                if out.len() == len {
                    break;
                }
            }
        }
    }
    out
}

pub fn sha256_hex(data: &[u8]) -> String {
    crate::config::encode_hex(&Sha256::digest(data))
}

/// `my title.html` → `my-title`, max 40 chars, never empty.
pub fn slugify(input: &str) -> String {
    let stem = input
        .rsplit_once('.')
        .map(|(s, ext)| {
            if ext.len() <= 5 && !s.is_empty() {
                s
            } else {
                input
            }
        })
        .unwrap_or(input);
    let mut out = String::new();
    let mut dash = true;
    for c in stem.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
        if out.len() >= 40 {
            break;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "file".to_string()
    } else {
        out
    }
}

pub fn file_public_id(slug: &str) -> String {
    format!("{}-{}", slug, random_base58(10))
}

pub fn api_key() -> String {
    format!("hk_{}", random_base58(32))
}

pub fn recipient_token() -> String {
    random_base58(22)
}

pub fn opaque_token() -> String {
    random_base58(32)
}

type HmacSha256 = Hmac<Sha256>;

pub fn hmac_b64(secret: &[u8], msg: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("hmac key");
    mac.update(msg);
    URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
}

pub fn hmac_verify(secret: &[u8], msg: &[u8], sig_b64: &str) -> bool {
    let Ok(sig) = URL_SAFE_NO_PAD.decode(sig_b64) else {
        return false;
    };
    let mut mac = HmacSha256::new_from_slice(secret).expect("hmac key");
    mac.update(msg);
    mac.verify_slice(&sig).is_ok()
}

pub fn b64url(data: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(data)
}

pub fn pkce_s256(verifier: &str) -> String {
    b64url(&Sha256::digest(verifier.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slugs() {
        assert_eq!(slugify("Acme Proposal.html"), "acme-proposal");
        assert_eq!(slugify("  ---  "), "file");
        assert_eq!(slugify("Q4 Pricing (v2).pdf"), "q4-pricing-v2");
        assert_eq!(slugify("a.very.dotted.name.md"), "a-very-dotted-name");
    }
    #[test]
    fn tokens() {
        assert_eq!(random_base58(10).len(), 10);
        assert_ne!(random_base58(10), random_base58(10));
        assert!(hmac_verify(b"k", b"m", &hmac_b64(b"k", b"m")));
        assert!(!hmac_verify(b"k", b"x", &hmac_b64(b"k", b"m")));
    }
}
