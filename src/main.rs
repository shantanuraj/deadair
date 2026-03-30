mod api;
mod auth;
mod db;
mod models;
mod pages;
mod poller;
mod reconciler;
mod session;
mod spotify;

use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use rusqlite::Connection;
use tokio::sync::RwLock;

use models::TokenData;

pub struct Config {
    pub spotify_client_id: String,
    pub spotify_client_secret: String,
    pub deadair_secret: Vec<u8>,
    pub db_path: String,
    pub port: u16,
    pub reconcile: bool,
    pub host: String,
}

impl Config {
    fn from_env() -> Result<Self> {
        let port = env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080u16);
        let host =
            env::var("DEADAIR_HOST").unwrap_or_else(|_| format!("http://localhost:{}", port));
        Ok(Self {
            spotify_client_id: env::var("SPOTIFY_CLIENT_ID")?,
            spotify_client_secret: env::var("SPOTIFY_CLIENT_SECRET")?,
            deadair_secret: env::var("DEADAIR_SECRET")?.into_bytes(),
            db_path: env::var("DEADAIR_DB").unwrap_or_else(|_| "deadair.db".into()),
            port,
            reconcile: env::var("DEADAIR_RECONCILE")
                .map(|v| v != "false")
                .unwrap_or(true),
            host,
        })
    }
}

pub struct AppState {
    pub config: Config,
    pub db: Mutex<Connection>,
    pub http: reqwest::Client,
    pub active_users: Mutex<HashMap<String, Arc<RwLock<TokenData>>>>,
}

pub struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        eprintln!("error: {}", self.0);
        (StatusCode::INTERNAL_SERVER_ERROR, format!("{}", self.0)).into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_env()?;
    let port = config.port;

    let conn = Connection::open(&config.db_path)?;
    db::create_tables(&conn)?;

    let state = Arc::new(AppState {
        config,
        db: Mutex::new(conn),
        http: reqwest::Client::new(),
        active_users: Mutex::new(HashMap::new()),
    });

    auth::restore_users(&state);

    let app = Router::new()
        .route("/", get(pages::landing))
        .route("/dashboard", get(pages::dashboard))
        .route("/auth/login", get(auth::login))
        .route("/callback", get(auth::callback))
        .route("/callback-manual", get(auth::callback_manual))
        .route("/auth/logout", get(auth::logout))
        .route("/api/events", get(api::events))
        .route("/api/stats", get(api::stats))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    eprintln!("deadair listening on :{}", port);
    axum::serve(listener, app).await?;

    Ok(())
}
