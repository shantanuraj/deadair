use anyhow::{bail, Result};

use crate::models::{
    PlaybackState, RecentlyPlayedResponse, TokenData, UserProfile,
};

const AUTH_URL: &str = "https://accounts.spotify.com/authorize";
const TOKEN_URL: &str = "https://accounts.spotify.com/api/token";
const API_BASE: &str = "https://api.spotify.com/v1";
const SCOPES: &str = "user-read-playback-state user-read-recently-played";

pub fn authorize_url(client_id: &str, redirect_uri: &str, state: &str) -> String {
    let params = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", SCOPES)
        .append_pair("state", state)
        .finish();
    format!("{}?{}", AUTH_URL, params)
}

pub async fn exchange_code(
    http: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
    code: &str,
    redirect_uri: &str,
) -> Result<TokenData> {
    let resp = http
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", client_id),
            ("client_secret", client_secret),
        ])
        .send()
        .await?;

    if !resp.status().is_success() {
        let body = resp.text().await?;
        bail!("token exchange failed: {}", body);
    }

    let mut token: TokenData = resp.json().await?;
    token.created_at = chrono::Utc::now().timestamp();
    Ok(token)
}

pub async fn refresh_token(
    http: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
    refresh: &str,
) -> Result<TokenData> {
    let resp = http
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh),
            ("client_id", client_id),
            ("client_secret", client_secret),
        ])
        .send()
        .await?;

    if !resp.status().is_success() {
        let body = resp.text().await?;
        bail!("token refresh failed: {}", body);
    }

    let mut token: TokenData = resp.json().await?;
    token.created_at = chrono::Utc::now().timestamp();
    Ok(token)
}

pub async fn get_profile(http: &reqwest::Client, access_token: &str) -> Result<UserProfile> {
    let resp = http
        .get(format!("{}/me", API_BASE))
        .bearer_auth(access_token)
        .send()
        .await?;

    if !resp.status().is_success() {
        bail!("profile fetch failed: {}", resp.status());
    }

    Ok(resp.json().await?)
}

pub async fn get_playback(
    http: &reqwest::Client,
    access_token: &str,
) -> Result<Option<PlaybackState>> {
    let resp = http
        .get(format!("{}/me/player", API_BASE))
        .bearer_auth(access_token)
        .send()
        .await?;

    if resp.status().as_u16() == 204 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        bail!("playback fetch failed: {}", resp.status());
    }

    Ok(Some(resp.json().await?))
}

pub async fn get_recently_played(
    http: &reqwest::Client,
    access_token: &str,
    limit: u32,
) -> Result<RecentlyPlayedResponse> {
    let resp = http
        .get(format!("{}/me/player/recently-played?limit={}", API_BASE, limit))
        .bearer_auth(access_token)
        .send()
        .await?;

    if !resp.status().is_success() {
        bail!("recently played fetch failed: {}", resp.status());
    }

    Ok(resp.json().await?)
}
