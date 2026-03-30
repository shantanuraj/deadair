use std::sync::Arc;

use axum::extract::{FromRequestParts, OptionalFromRequestParts};
use axum::http::request::Parts;
use axum::response::Redirect;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::AppState;

pub struct Session {
    pub user_id: String,
}

pub fn sign_cookie(user_id: &str, secret: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
    mac.update(user_id.as_bytes());
    let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    format!("{}.{}", user_id, sig)
}

pub fn verify_cookie(value: &str, secret: &[u8]) -> Option<String> {
    let (user_id, sig_b64) = value.split_once('.')?;
    let sig = URL_SAFE_NO_PAD.decode(sig_b64).ok()?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
    mac.update(user_id.as_bytes());
    mac.verify_slice(&sig).ok()?;
    Some(user_id.to_string())
}

fn extract_cookie(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .map(|s| s.trim())
        .find(|s| s.starts_with(&format!("{}=", name)))?
        .split_once('=')
        .map(|(_, v)| v.to_string())
}

impl FromRequestParts<Arc<AppState>> for Session {
    type Rejection = Redirect;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let cookie = extract_cookie(&parts.headers, "deadair_session")
            .and_then(|v| verify_cookie(&v, &state.config.deadair_secret));

        match cookie {
            Some(user_id) => Ok(Session { user_id }),
            None => Err(Redirect::to("/")),
        }
    }
}

impl OptionalFromRequestParts<Arc<AppState>> for Session {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Option<Self>, Self::Rejection> {
        let cookie = extract_cookie(&parts.headers, "deadair_session")
            .and_then(|v| verify_cookie(&v, &state.config.deadair_secret));

        Ok(cookie.map(|user_id| Session { user_id }))
    }
}

pub fn set_cookie_header(user_id: &str, secret: &[u8]) -> String {
    let value = sign_cookie(user_id, secret);
    format!(
        "deadair_session={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        value,
        60 * 60 * 24 * 30
    )
}

pub fn clear_cookie_header() -> String {
    "deadair_session=; Path=/; HttpOnly; Max-Age=0".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"test-secret-key-for-deadair";

    #[test]
    fn sign_and_verify() {
        let cookie = sign_cookie("user123", SECRET);
        assert!(cookie.starts_with("user123."));
        let result = verify_cookie(&cookie, SECRET);
        assert_eq!(result, Some("user123".to_string()));
    }

    #[test]
    fn reject_tampered() {
        let cookie = sign_cookie("user123", SECRET);
        let tampered = cookie.replace("user123", "admin");
        assert_eq!(verify_cookie(&tampered, SECRET), None);
    }

    #[test]
    fn reject_wrong_secret() {
        let cookie = sign_cookie("user123", SECRET);
        assert_eq!(verify_cookie(&cookie, b"wrong-secret"), None);
    }

    #[test]
    fn reject_garbage() {
        assert_eq!(verify_cookie("not-a-cookie", SECRET), None);
        assert_eq!(verify_cookie("", SECRET), None);
    }

    #[test]
    fn extract_from_header() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("cookie", "deadair_session=abc.xyz; other=val".parse().unwrap());
        let result = extract_cookie(&headers, "deadair_session");
        assert_eq!(result, Some("abc.xyz".to_string()));
    }

    #[test]
    fn extract_missing() {
        let headers = axum::http::HeaderMap::new();
        assert_eq!(extract_cookie(&headers, "deadair_session"), None);
    }
}
