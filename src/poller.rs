use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::Connection;
use tokio::sync::RwLock;

use crate::models::{PlaybackState, TokenData};
use crate::{db, spotify, AppState};

const SKIP_THRESHOLD_MS: i64 = 10_000;

#[derive(Default)]
pub struct TrackingState {
    pub current_track_id: Option<String>,
    pub max_progress_ms: i64,
    pub duration_ms: i64,
    pub open_classification_id: Option<i64>,
}

pub async fn run(user_id: String, token: Arc<RwLock<TokenData>>, state: Arc<AppState>) {
    let mut tracking = TrackingState::default();
    let mut interval = tokio::time::interval(Duration::from_secs(5));

    loop {
        interval.tick().await;

        let access_token = { token.read().await.access_token.clone() };

        match spotify::get_playback(&state.http, &access_token).await {
            Ok(playback) => {
                if let Err(e) =
                    process_poll(&mut tracking, &state.db, &user_id, playback.as_ref())
                {
                    eprintln!("poll error for {}: {}", user_id, e);
                }
            }
            Err(e) => eprintln!("spotify error for {}: {}", user_id, e),
        }
    }
}

pub fn process_poll(
    tracking: &mut TrackingState,
    db_mutex: &Mutex<Connection>,
    user_id: &str,
    playback: Option<&PlaybackState>,
) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp();

    let active_track = playback.and_then(|p| p.item.as_ref().map(|item| (p, item)));

    match active_track {
        Some((pb, item)) => {
            let progress = pb.progress_ms.unwrap_or(0);

            let is_new = match &tracking.current_track_id {
                None => true,
                Some(id) if id != &item.id => true,
                Some(_) if pb.is_playing && progress < tracking.max_progress_ms - 10_000 => true,
                _ => false,
            };

            if is_new {
                close_current(tracking, db_mutex, now)?;

                let context_uri = pb.context.as_ref().and_then(|c| c.uri.as_deref());
                let class_id = {
                    let conn = db_mutex.lock().unwrap();
                    db::open_classification(
                        &conn,
                        user_id,
                        &item.id,
                        &item.name,
                        &item.artist_names(),
                        &item.album.name,
                        now,
                        item.duration_ms,
                        context_uri,
                    )?
                };

                tracking.current_track_id = Some(item.id.clone());
                tracking.max_progress_ms = progress;
                tracking.duration_ms = item.duration_ms;
                tracking.open_classification_id = Some(class_id);
            } else if pb.is_playing {
                tracking.max_progress_ms = tracking.max_progress_ms.max(progress);
            }

            {
                let conn = db_mutex.lock().unwrap();
                insert_event(&conn, user_id, pb, item, progress, now)?;
            }
        }
        None => {
            close_current(tracking, db_mutex, now)?;
        }
    }

    Ok(())
}

fn close_current(
    tracking: &mut TrackingState,
    db_mutex: &Mutex<Connection>,
    now: i64,
) -> anyhow::Result<()> {
    if let Some(class_id) = tracking.open_classification_id.take() {
        let skipped = tracking.max_progress_ms < tracking.duration_ms - SKIP_THRESHOLD_MS;
        let conn = db_mutex.lock().unwrap();
        db::close_classification(&conn, class_id, now, tracking.max_progress_ms, skipped)?;
    }
    tracking.current_track_id = None;
    tracking.max_progress_ms = 0;
    tracking.duration_ms = 0;
    Ok(())
}

fn insert_event(
    conn: &Connection,
    user_id: &str,
    pb: &PlaybackState,
    item: &crate::models::TrackItem,
    progress: i64,
    now: i64,
) -> anyhow::Result<()> {
    db::insert_event(
        conn,
        user_id,
        &item.id,
        &item.name,
        &item.artist_names(),
        &item.album.name,
        item.duration_ms,
        progress,
        pb.is_playing,
        pb.shuffle_state.unwrap_or(false),
        pb.repeat_state.as_deref().unwrap_or("off"),
        pb.context.as_ref().and_then(|c| c.uri.as_deref()),
        pb.device.as_ref().and_then(|d| d.name.as_deref()),
        now,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::*;

    fn test_db() -> Mutex<Connection> {
        let conn = Connection::open_in_memory().unwrap();
        db::create_tables(&conn).unwrap();
        db::upsert_user(&conn, "u1", "Test", "{}").unwrap();
        Mutex::new(conn)
    }

    fn playback(track_id: &str, progress: i64, duration: i64, playing: bool) -> PlaybackState {
        PlaybackState {
            device: Some(Device {
                id: None,
                name: Some("Test".into()),
                device_type: None,
            }),
            shuffle_state: Some(false),
            repeat_state: Some("off".into()),
            timestamp: Some(0),
            context: None,
            progress_ms: Some(progress),
            item: Some(TrackItem {
                id: track_id.into(),
                name: format!("Track {}", track_id),
                duration_ms: duration,
                artists: vec![ArtistRef {
                    id: "a1".into(),
                    name: "Artist".into(),
                }],
                album: AlbumRef {
                    id: "al1".into(),
                    name: "Album".into(),
                },
                uri: format!("spotify:track:{}", track_id),
            }),
            currently_playing_type: Some("track".into()),
            is_playing: playing,
        }
    }

    #[test]
    fn new_track_opens_classification() {
        let db = test_db();
        let mut state = TrackingState::default();

        process_poll(&mut state, &db, "u1", Some(&playback("t1", 5_000, 240_000, true))).unwrap();

        assert_eq!(state.current_track_id, Some("t1".into()));
        assert!(state.open_classification_id.is_some());
    }

    #[test]
    fn track_change_closes_previous() {
        let db = test_db();
        let mut state = TrackingState::default();

        process_poll(&mut state, &db, "u1", Some(&playback("t1", 120_000, 240_000, true))).unwrap();
        process_poll(&mut state, &db, "u1", Some(&playback("t2", 5_000, 200_000, true))).unwrap();

        let conn = db.lock().unwrap();
        let classes = db::classifications_in_range(&conn, "u1", 0, i64::MAX).unwrap();
        assert_eq!(classes.len(), 2);
        assert!(classes[0].ended_at.is_some());
        assert_eq!(classes[0].skipped, Some(true));
        assert_eq!(classes[0].listened_ms, 120_000);
        assert!(classes[1].ended_at.is_none());
    }

    #[test]
    fn played_in_full() {
        let db = test_db();
        let mut state = TrackingState::default();

        process_poll(&mut state, &db, "u1", Some(&playback("t1", 5_000, 200_000, true))).unwrap();
        state.max_progress_ms = 195_000;
        process_poll(&mut state, &db, "u1", Some(&playback("t2", 0, 180_000, true))).unwrap();

        let conn = db.lock().unwrap();
        let classes = db::classifications_in_range(&conn, "u1", 0, i64::MAX).unwrap();
        assert_eq!(classes[0].skipped, Some(false));
        assert_eq!(classes[0].listened_ms, 195_000);
    }

    #[test]
    fn nothing_playing_closes() {
        let db = test_db();
        let mut state = TrackingState::default();

        process_poll(&mut state, &db, "u1", Some(&playback("t1", 60_000, 240_000, true))).unwrap();
        process_poll(&mut state, &db, "u1", None).unwrap();

        assert!(state.current_track_id.is_none());
        assert!(state.open_classification_id.is_none());

        let conn = db.lock().unwrap();
        let classes = db::classifications_in_range(&conn, "u1", 0, i64::MAX).unwrap();
        assert_eq!(classes.len(), 1);
        assert!(classes[0].ended_at.is_some());
    }

    #[test]
    fn repeat_detection() {
        let db = test_db();
        let mut state = TrackingState::default();

        process_poll(&mut state, &db, "u1", Some(&playback("t1", 180_000, 200_000, true))).unwrap();
        state.max_progress_ms = 180_000;
        process_poll(&mut state, &db, "u1", Some(&playback("t1", 5_000, 200_000, true))).unwrap();

        let conn = db.lock().unwrap();
        let classes = db::classifications_in_range(&conn, "u1", 0, i64::MAX).unwrap();
        assert_eq!(classes.len(), 2);
        assert!(classes[0].ended_at.is_some());
    }

    #[test]
    fn pause_does_not_close() {
        let db = test_db();
        let mut state = TrackingState::default();

        process_poll(&mut state, &db, "u1", Some(&playback("t1", 60_000, 240_000, true))).unwrap();
        let paused = playback("t1", 60_000, 240_000, false);
        process_poll(&mut state, &db, "u1", Some(&paused)).unwrap();

        assert!(state.open_classification_id.is_some());
        assert_eq!(state.current_track_id, Some("t1".into()));
    }

    #[test]
    fn progress_tracks_max() {
        let db = test_db();
        let mut state = TrackingState::default();

        process_poll(&mut state, &db, "u1", Some(&playback("t1", 50_000, 240_000, true))).unwrap();
        process_poll(&mut state, &db, "u1", Some(&playback("t1", 55_000, 240_000, true))).unwrap();
        process_poll(&mut state, &db, "u1", Some(&playback("t1", 60_000, 240_000, true))).unwrap();

        assert_eq!(state.max_progress_ms, 60_000);
    }
}
