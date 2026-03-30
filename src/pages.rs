use std::sync::Arc;

use axum::extract::State;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Redirect, Response};

use crate::session::Session;
use crate::{db, AppResult, AppState};

const STYLE: &str = "body{font-family:'SF Mono','Menlo','Monaco','Courier New',monospace;\
background:#fff;color:#000;max-width:40em;margin:4em auto;padding:0 1em;line-height:1.6}\
a{color:#000}hr{border:none;border-top:1px solid #000;margin:2em 0}\
h1{font-weight:400;letter-spacing:0.05em}";

pub async fn landing(session: Option<Session>) -> Response {
    if session.is_some() {
        return Redirect::to("/dashboard").into_response();
    }

    (
        [(CONTENT_TYPE, "text/html")],
        format!(
            r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>deadair</title><style>{STYLE}</style></head>
<body>
<h1>deadair</h1>
<p>spotify playback tracker</p>
<hr>
<p><a href="/auth/login">login with spotify</a></p>
</body></html>"#
        ),
    )
        .into_response()
}

pub async fn dashboard(
    session: Session,
    State(state): State<Arc<AppState>>,
) -> AppResult<Response> {
    let (display_name, stats) = {
        let conn = state.db.lock().unwrap();
        let name = db::get_display_name(&conn, &session.user_id)?
            .unwrap_or_else(|| session.user_id.clone());
        let stats = db::get_stats(&conn, &session.user_id)?;
        (name, stats)
    };

    let skip_pct = format!("{:.1}", stats.skip_rate * 100.0);

    let top_skipped_rows: String = stats
        .top_skipped
        .iter()
        .map(|t| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
                t.track_name, t.artist_name, t.count
            )
        })
        .collect();

    let top_skipped_table = if top_skipped_rows.is_empty() {
        String::new()
    } else {
        format!(
            "<h2>top skipped</h2><table><tr><th>track</th><th>artist</th><th>times</th></tr>{}</table>",
            top_skipped_rows
        )
    };

    Ok((
        [(CONTENT_TYPE, "text/html")],
        format!(
            r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>deadair</title>
<style>{STYLE}
table{{border-collapse:collapse;width:100%}}th,td{{text-align:left;padding:0.3em 1em 0.3em 0}}
th{{border-bottom:1px solid #000}}h2{{font-weight:400;font-size:1em;margin-top:2em}}</style></head>
<body>
<h1>deadair</h1>
<p>{display_name}</p>
<hr>
<p>{total} listens &middot; {skipped} skipped ({skip_pct}%) &middot; {completed} played</p>
{top_skipped_table}
<hr>
<p><a href="/api/events?format=csv">export csv</a> &middot; <a href="/api/events">export json</a> &middot; <a href="/api/stats">stats</a></p>
<hr>
<p><a href="/auth/logout">logout</a></p>
</body></html>"#,
            total = stats.total_listens,
            skipped = stats.skipped,
            completed = stats.completed,
        ),
    )
        .into_response())
}
