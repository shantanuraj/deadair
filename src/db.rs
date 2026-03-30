use anyhow::Result;
use rusqlite::Connection;

use crate::models::{ArtistSkipRate, Classification, Stats, TrackCount};

pub fn create_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS users (
            spotify_id   TEXT    PRIMARY KEY,
            display_name TEXT    NOT NULL,
            tokens       TEXT    NOT NULL,
            created_at   INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS playback_events (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id      TEXT    NOT NULL,
            track_id     TEXT    NOT NULL,
            track_name   TEXT    NOT NULL,
            artist_name  TEXT    NOT NULL,
            album_name   TEXT    NOT NULL,
            duration_ms  INTEGER NOT NULL,
            progress_ms  INTEGER NOT NULL,
            is_playing   BOOLEAN NOT NULL,
            shuffle      BOOLEAN NOT NULL,
            repeat_state TEXT    NOT NULL,
            context_uri  TEXT,
            device_name  TEXT,
            polled_at    INTEGER NOT NULL,
            FOREIGN KEY (user_id) REFERENCES users (spotify_id)
        );
        CREATE TABLE IF NOT EXISTS classifications (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id      TEXT    NOT NULL,
            track_id     TEXT    NOT NULL,
            track_name   TEXT    NOT NULL,
            artist_name  TEXT    NOT NULL,
            album_name   TEXT    NOT NULL,
            started_at   INTEGER NOT NULL,
            ended_at     INTEGER,
            duration_ms  INTEGER NOT NULL,
            listened_ms  INTEGER NOT NULL DEFAULT 0,
            skipped      BOOLEAN,
            context_uri  TEXT,
            FOREIGN KEY (user_id) REFERENCES users (spotify_id)
        );",
    )?;
    Ok(())
}

pub fn upsert_user(
    conn: &Connection,
    spotify_id: &str,
    display_name: &str,
    tokens_json: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO users (spotify_id, display_name, tokens, created_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(spotify_id) DO UPDATE SET display_name = ?2, tokens = ?3",
        (spotify_id, display_name, tokens_json, chrono::Utc::now().timestamp()),
    )?;
    Ok(())
}

pub fn get_all_users(conn: &Connection) -> Result<Vec<(String, String, String)>> {
    let mut stmt = conn.prepare("SELECT spotify_id, display_name, tokens FROM users")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn update_tokens(conn: &Connection, spotify_id: &str, tokens_json: &str) -> Result<()> {
    conn.execute(
        "UPDATE users SET tokens = ?1 WHERE spotify_id = ?2",
        (tokens_json, spotify_id),
    )?;
    Ok(())
}

pub fn get_display_name(conn: &Connection, spotify_id: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT display_name FROM users WHERE spotify_id = ?1")?;
    let name = stmt
        .query_row((spotify_id,), |row| row.get(0))
        .ok();
    Ok(name)
}

#[allow(clippy::too_many_arguments)]
pub fn insert_event(
    conn: &Connection,
    user_id: &str,
    track_id: &str,
    track_name: &str,
    artist_name: &str,
    album_name: &str,
    duration_ms: i64,
    progress_ms: i64,
    is_playing: bool,
    shuffle: bool,
    repeat_state: &str,
    context_uri: Option<&str>,
    device_name: Option<&str>,
    polled_at: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO playback_events
         (user_id, track_id, track_name, artist_name, album_name, duration_ms, progress_ms,
          is_playing, shuffle, repeat_state, context_uri, device_name, polled_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        (
            user_id,
            track_id,
            track_name,
            artist_name,
            album_name,
            duration_ms,
            progress_ms,
            is_playing,
            shuffle,
            repeat_state,
            context_uri,
            device_name,
            polled_at,
        ),
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn open_classification(
    conn: &Connection,
    user_id: &str,
    track_id: &str,
    track_name: &str,
    artist_name: &str,
    album_name: &str,
    started_at: i64,
    duration_ms: i64,
    context_uri: Option<&str>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO classifications
         (user_id, track_id, track_name, artist_name, album_name, started_at, duration_ms, listened_ms, context_uri)
         VALUES (?1,?2,?3,?4,?5,?6,?7,0,?8)",
        (
            user_id,
            track_id,
            track_name,
            artist_name,
            album_name,
            started_at,
            duration_ms,
            context_uri,
        ),
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn close_classification(
    conn: &Connection,
    id: i64,
    ended_at: i64,
    listened_ms: i64,
    skipped: bool,
) -> Result<()> {
    conn.execute(
        "UPDATE classifications SET ended_at = ?1, listened_ms = ?2, skipped = ?3 WHERE id = ?4",
        (ended_at, listened_ms, skipped, id),
    )?;
    Ok(())
}

pub fn classification_exists_near(
    conn: &Connection,
    user_id: &str,
    track_id: &str,
    around: i64,
    tolerance: i64,
) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM classifications
         WHERE user_id = ?1 AND track_id = ?2 AND ABS(started_at - ?3) <= ?4",
        (user_id, track_id, around, tolerance),
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub fn classifications_in_range(
    conn: &Connection,
    user_id: &str,
    from: i64,
    to: i64,
) -> Result<Vec<Classification>> {
    let mut stmt = conn.prepare(
        "SELECT id, user_id, track_id, track_name, artist_name, album_name,
                started_at, ended_at, duration_ms, listened_ms, skipped, context_uri
         FROM classifications
         WHERE user_id = ?1 AND started_at >= ?2 AND started_at <= ?3
         ORDER BY started_at ASC",
    )?;
    let rows = stmt.query_map((user_id, from, to), |row| {
        Ok(Classification {
            id: row.get(0)?,
            user_id: row.get(1)?,
            track_id: row.get(2)?,
            track_name: row.get(3)?,
            artist_name: row.get(4)?,
            album_name: row.get(5)?,
            started_at: row.get(6)?,
            ended_at: row.get(7)?,
            duration_ms: row.get(8)?,
            listened_ms: row.get(9)?,
            skipped: row.get(10)?,
            context_uri: row.get(11)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn get_stats(conn: &Connection, user_id: &str) -> Result<Stats> {
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM classifications WHERE user_id = ?1 AND ended_at IS NOT NULL",
        (user_id,),
        |row| row.get(0),
    )?;
    let skipped: i64 = conn.query_row(
        "SELECT COUNT(*) FROM classifications WHERE user_id = ?1 AND skipped = 1",
        (user_id,),
        |row| row.get(0),
    )?;
    let completed = total - skipped;
    let skip_rate = if total > 0 {
        skipped as f64 / total as f64
    } else {
        0.0
    };

    let mut stmt = conn.prepare(
        "SELECT track_name, artist_name, COUNT(*) as cnt
         FROM classifications WHERE user_id = ?1 AND skipped = 1
         GROUP BY track_id ORDER BY cnt DESC LIMIT 10",
    )?;
    let top_skipped = stmt
        .query_map((user_id,), |row| {
            Ok(TrackCount {
                track_name: row.get(0)?,
                artist_name: row.get(1)?,
                count: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut stmt = conn.prepare(
        "SELECT artist_name, COUNT(*) as total,
                SUM(CASE WHEN skipped = 1 THEN 1 ELSE 0 END) as skip_count
         FROM classifications WHERE user_id = ?1 AND ended_at IS NOT NULL
         GROUP BY artist_name HAVING total >= 3
         ORDER BY (CAST(skip_count AS REAL) / total) DESC LIMIT 10",
    )?;
    let top_artists = stmt
        .query_map((user_id,), |row| {
            let t: i64 = row.get(1)?;
            let s: i64 = row.get(2)?;
            Ok(ArtistSkipRate {
                artist_name: row.get(0)?,
                total: t,
                skipped: s,
                skip_rate: if t > 0 { s as f64 / t as f64 } else { 0.0 },
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Stats {
        total_listens: total,
        completed,
        skipped,
        skip_rate,
        top_skipped,
        top_artists_by_skip_rate: top_artists,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_tables(&conn).unwrap();
        conn
    }

    #[test]
    fn upsert_and_get_users() {
        let conn = test_conn();
        upsert_user(&conn, "user1", "Alice", r#"{"access_token":"x"}"#).unwrap();
        let users = get_all_users(&conn).unwrap();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].0, "user1");
        assert_eq!(users[0].1, "Alice");

        upsert_user(&conn, "user1", "Alice Updated", r#"{"access_token":"y"}"#).unwrap();
        let users = get_all_users(&conn).unwrap();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].1, "Alice Updated");
    }

    #[test]
    fn classification_lifecycle() {
        let conn = test_conn();
        upsert_user(&conn, "u1", "Test", "{}").unwrap();

        let id = open_classification(&conn, "u1", "t1", "Song", "Artist", "Album", 1000, 240_000, None).unwrap();
        assert!(id > 0);

        close_classification(&conn, id, 1240, 200_000, true).unwrap();

        let classes = classifications_in_range(&conn, "u1", 0, 2000).unwrap();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].skipped, Some(true));
        assert_eq!(classes[0].listened_ms, 200_000);
        assert_eq!(classes[0].ended_at, Some(1240));
    }

    #[test]
    fn classification_exists_near_check() {
        let conn = test_conn();
        upsert_user(&conn, "u1", "Test", "{}").unwrap();
        open_classification(&conn, "u1", "t1", "Song", "Artist", "Album", 1000, 240_000, None).unwrap();

        assert!(classification_exists_near(&conn, "u1", "t1", 1015, 30).unwrap());
        assert!(!classification_exists_near(&conn, "u1", "t1", 2000, 30).unwrap());
        assert!(!classification_exists_near(&conn, "u1", "t2", 1000, 30).unwrap());
    }

    #[test]
    fn stats_computation() {
        let conn = test_conn();
        upsert_user(&conn, "u1", "Test", "{}").unwrap();

        let id1 = open_classification(&conn, "u1", "t1", "Skip Me", "A", "Al", 100, 240_000, None).unwrap();
        close_classification(&conn, id1, 200, 60_000, true).unwrap();

        let id2 = open_classification(&conn, "u1", "t2", "Play Me", "A", "Al", 300, 200_000, None).unwrap();
        close_classification(&conn, id2, 500, 195_000, false).unwrap();

        let id3 = open_classification(&conn, "u1", "t1", "Skip Me", "A", "Al", 600, 240_000, None).unwrap();
        close_classification(&conn, id3, 700, 50_000, true).unwrap();

        let stats = get_stats(&conn, "u1").unwrap();
        assert_eq!(stats.total_listens, 3);
        assert_eq!(stats.skipped, 2);
        assert_eq!(stats.completed, 1);
        assert_eq!(stats.top_skipped.len(), 1);
        assert_eq!(stats.top_skipped[0].count, 2);
    }
}
