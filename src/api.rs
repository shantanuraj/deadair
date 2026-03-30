use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::response::{IntoResponse, Json, Response};
use serde::Deserialize;

use crate::session::Session;
use crate::{db, AppResult, AppState};

#[derive(Deserialize)]
pub struct EventsParams {
    pub format: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub last: Option<String>,
    pub limit: Option<i64>,
}

fn resolve_window(params: &EventsParams) -> anyhow::Result<(i64, i64)> {
    let now = chrono::Utc::now().timestamp();

    if let Some(ref last) = params.last {
        let (num, unit) = last.split_at(last.len() - 1);
        let n: i64 = num.parse()?;
        let secs = match unit {
            "m" => n * 60,
            "h" => n * 3600,
            "d" => n * 86400,
            _ => anyhow::bail!("unknown duration unit"),
        };
        return Ok((now - secs, now));
    }

    if let Some(ref since) = params.since {
        let from = chrono::NaiveDate::parse_from_str(since, "%Y-%m-%d")?
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp();
        let to = match &params.until {
            Some(until) => chrono::NaiveDate::parse_from_str(until, "%Y-%m-%d")?
                .and_hms_opt(23, 59, 59)
                .unwrap()
                .and_utc()
                .timestamp(),
            None => now,
        };
        return Ok((from, to));
    }

    Ok((now - 86400, now))
}

pub async fn events(
    session: Session,
    State(state): State<Arc<AppState>>,
    Query(params): Query<EventsParams>,
) -> AppResult<Response> {
    let (from, to) = resolve_window(&params)?;
    let classifications = {
        let conn = state.db.lock().unwrap();
        db::classifications_in_range(&conn, &session.user_id, from, to)?
    };

    let format = params.format.as_deref().unwrap_or("json");

    if format == "csv" {
        let mut wtr = csv::Writer::from_writer(Vec::new());
        for c in &classifications {
            wtr.serialize(c)?;
        }
        let body = String::from_utf8(wtr.into_inner()?)?;
        Ok((
            [
                (CONTENT_TYPE, "text/csv"),
                (CONTENT_DISPOSITION, "attachment; filename=\"deadair-events.csv\""),
            ],
            body,
        )
            .into_response())
    } else {
        Ok(Json(&classifications).into_response())
    }
}

pub async fn playback(
    session: Session,
    State(state): State<Arc<AppState>>,
    Query(params): Query<EventsParams>,
) -> AppResult<Response> {
    let (from, to) = resolve_window(&params)?;
    let limit = match params.limit {
        Some(0) => -1,
        Some(n) => n,
        None => 1000,
    };
    let events = {
        let conn = state.db.lock().unwrap();
        db::playback_events_in_range(&conn, &session.user_id, from, to, limit)?
    };

    let format = params.format.as_deref().unwrap_or("json");

    if format == "csv" {
        let mut wtr = csv::Writer::from_writer(Vec::new());
        for e in &events {
            wtr.serialize(e)?;
        }
        let body = String::from_utf8(wtr.into_inner()?)?;
        Ok((
            [
                (CONTENT_TYPE, "text/csv"),
                (CONTENT_DISPOSITION, "attachment; filename=\"deadair-playback.csv\""),
            ],
            body,
        )
            .into_response())
    } else {
        Ok(Json(&events).into_response())
    }
}

pub async fn stats(
    session: Session,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<crate::models::Stats>> {
    let stats = {
        let conn = state.db.lock().unwrap();
        db::get_stats(&conn, &session.user_id)?
    };
    Ok(Json(stats))
}
