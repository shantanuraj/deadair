use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::header::SET_COOKIE;
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::session::{clear_cookie_header, set_cookie_header};
use crate::{db, spotify, AppState};

#[derive(Deserialize)]
pub struct LoginParams {
    pub manual: Option<bool>,
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Query(params): Query<LoginParams>,
) -> Response {
    let redirect_uri = format!("{}/callback", state.config.host);
    let csrf = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(chrono::Utc::now().timestamp().to_le_bytes());
    let url = spotify::authorize_url(&state.config.spotify_client_id, &redirect_uri, &csrf);

    if params.manual.unwrap_or(false) {
        Html(format!(
            r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>deadair</title>
<style>body{{font-family:'SF Mono',monospace;max-width:40em;margin:4em auto;padding:0 1em}}
input{{width:100%;padding:0.5em;font-family:inherit;margin:0.5em 0}}a{{color:#000}}</style>
</head><body>
<h1>deadair</h1>
<p><a href="{url}">authorize with spotify</a></p>
<p>after authorizing, paste the redirect URL below:</p>
<form method="get" action="/callback-manual">
<input type="text" name="url" placeholder="paste redirect URL here" autofocus>
<button type="submit">submit</button>
</form>
</body></html>"#
        ))
        .into_response()
    } else {
        Redirect::to(&url).into_response()
    }
}

use base64::Engine;

#[derive(Deserialize)]
pub struct CallbackParams {
    pub code: String,
    #[allow(dead_code)]
    pub state: Option<String>,
}

pub async fn callback(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CallbackParams>,
) -> Result<Response, crate::AppError> {
    let redirect_uri = format!("{}/callback", state.config.host);

    let token = spotify::exchange_code(
        &state.http,
        &state.config.spotify_client_id,
        &state.config.spotify_client_secret,
        &params.code,
        &redirect_uri,
    )
    .await?;

    let profile = spotify::get_profile(&state.http, &token.access_token).await?;
    let display_name = profile.display_name.unwrap_or_else(|| profile.id.clone());
    let tokens_json = serde_json::to_string(&token)?;

    {
        let conn = state.db.lock().unwrap();
        db::upsert_user(&conn, &profile.id, &display_name, &tokens_json)?;
    }

    spawn_user_tasks(&state, &profile.id, token);

    let cookie = set_cookie_header(&profile.id, &state.config.deadair_secret);
    Ok(([(SET_COOKIE, cookie)], Redirect::to("/dashboard")).into_response())
}

#[derive(Deserialize)]
pub struct ManualCallbackParams {
    pub url: String,
}

pub async fn callback_manual(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ManualCallbackParams>,
) -> Result<Response, crate::AppError> {
    let parsed = url::Url::parse(&params.url)
        .or_else(|_| url::Url::parse(&format!("http://localhost{}", params.url)))?;

    let code = parsed
        .query_pairs()
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.to_string())
        .ok_or_else(|| anyhow::anyhow!("no code in URL"))?;

    let fake_params = CallbackParams {
        code,
        state: None,
    };

    callback(State(state), Query(fake_params)).await
}

pub async fn logout(State(_state): State<Arc<AppState>>) -> Response {
    let cookie = clear_cookie_header();
    ([(SET_COOKIE, cookie)], Redirect::to("/")).into_response()
}

fn spawn_user_tasks(state: &Arc<AppState>, user_id: &str, token: crate::models::TokenData) {
    let already_active = state
        .active_users
        .lock()
        .unwrap()
        .contains_key(user_id);

    if already_active {
        return;
    }

    let token = Arc::new(RwLock::new(token));
    state
        .active_users
        .lock()
        .unwrap()
        .insert(user_id.to_string(), token.clone());

    let user_id_owned = user_id.to_string();

    let s = state.clone();
    let t = token.clone();
    let u = user_id_owned.clone();
    tokio::spawn(async move { crate::poller::run(u, t, s).await });

    if state.config.reconcile {
        let s = state.clone();
        let t = token.clone();
        let u = user_id_owned.clone();
        tokio::spawn(async move { crate::reconciler::run(u, t, s).await });
    }

    let s = state.clone();
    let t = token.clone();
    let u = user_id_owned.clone();
    tokio::spawn(async move { run_refresher(u, t, s).await });
}

async fn run_refresher(
    user_id: String,
    token: Arc<RwLock<crate::models::TokenData>>,
    state: Arc<AppState>,
) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(20 * 60));
    interval.tick().await;

    loop {
        interval.tick().await;

        let refresh = {
            let t = token.read().await;
            t.refresh_token.clone()
        };

        let Some(refresh) = refresh else { continue };

        match spotify::refresh_token(
            &state.http,
            &state.config.spotify_client_id,
            &state.config.spotify_client_secret,
            &refresh,
        )
        .await
        {
            Ok(mut new_token) => {
                if new_token.refresh_token.is_none() {
                    new_token.refresh_token = Some(refresh);
                }
                let json = serde_json::to_string(&new_token).unwrap();
                {
                    let conn = state.db.lock().unwrap();
                    let _ = db::update_tokens(&conn, &user_id, &json);
                }
                let mut t = token.write().await;
                *t = new_token;
                eprintln!("refreshed token for {}", user_id);
            }
            Err(e) => eprintln!("refresh error for {}: {}", user_id, e),
        }
    }
}

pub fn restore_users(state: &Arc<AppState>) {
    let users = {
        let conn = state.db.lock().unwrap();
        db::get_all_users(&conn).unwrap_or_default()
    };

    for (spotify_id, _display_name, tokens_json) in users {
        if let Ok(token) = serde_json::from_str::<crate::models::TokenData>(&tokens_json) {
            spawn_user_tasks(state, &spotify_id, token);
            eprintln!("restored polling for {}", spotify_id);
        }
    }
}
