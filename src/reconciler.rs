use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;

use crate::models::TokenData;
use crate::{db, spotify, AppState};

pub async fn run(user_id: String, token: Arc<RwLock<TokenData>>, state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));

    loop {
        interval.tick().await;

        let access_token = { token.read().await.access_token.clone() };

        match spotify::get_recently_played(&state.http, &access_token, 50).await {
            Ok(recent) => {
                if let Err(e) = backfill(&state, &user_id, &recent.items) {
                    eprintln!("reconciler error for {}: {}", user_id, e);
                }
            }
            Err(e) => eprintln!("reconciler fetch error for {}: {}", user_id, e),
        }
    }
}

fn backfill(
    state: &AppState,
    user_id: &str,
    items: &[crate::models::RecentlyPlayedItem],
) -> anyhow::Result<()> {
    if items.is_empty() {
        return Ok(());
    }

    let mut sorted: Vec<_> = items.iter().collect();
    sorted.sort_by_key(|item| &item.played_at);

    let conn = state.db.lock().unwrap();

    for (i, item) in sorted.iter().enumerate() {
        let played_at = chrono::DateTime::parse_from_rfc3339(&item.played_at)
            .map(|dt| dt.timestamp())
            .unwrap_or(0);

        let listened_ms = if i + 1 < sorted.len() {
            let next_played_at = chrono::DateTime::parse_from_rfc3339(&sorted[i + 1].played_at)
                .map(|dt| dt.timestamp())
                .unwrap_or(0);
            let gap_ms = (next_played_at - played_at) * 1000;
            gap_ms.min(item.track.duration_ms)
        } else {
            item.track.duration_ms
        };

        let started_at = played_at - (listened_ms / 1000);

        if db::classification_exists_near(&conn, user_id, &item.track.id, played_at, 30)? {
            continue;
        }

        let skipped = listened_ms < item.track.duration_ms - 10_000;
        let artist_name = item.track.artist_names();
        let context_uri = item.context.as_ref().and_then(|c| c.uri.as_deref());

        let id = db::open_classification(
            &conn,
            user_id,
            &item.track.id,
            &item.track.name,
            &artist_name,
            &item.track.album.name,
            started_at,
            item.track.duration_ms,
            context_uri,
        )?;
        db::close_classification(&conn, id, played_at, listened_ms, skipped)?;
    }

    Ok(())
}
