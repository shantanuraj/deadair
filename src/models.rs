use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PlaybackState {
    pub device: Option<Device>,
    pub shuffle_state: Option<bool>,
    pub repeat_state: Option<String>,
    pub timestamp: Option<i64>,
    pub context: Option<SpotifyContext>,
    pub progress_ms: Option<i64>,
    pub item: Option<TrackItem>,
    pub currently_playing_type: Option<String>,
    pub is_playing: bool,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Device {
    pub id: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub device_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct TrackItem {
    pub id: String,
    pub name: String,
    pub duration_ms: i64,
    pub artists: Vec<ArtistRef>,
    pub album: AlbumRef,
    pub uri: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ArtistRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct AlbumRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct SpotifyContext {
    pub uri: Option<String>,
    #[serde(rename = "type")]
    pub context_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RecentlyPlayedResponse {
    pub items: Vec<RecentlyPlayedItem>,
}

#[derive(Debug, Deserialize)]
pub struct RecentlyPlayedItem {
    pub track: TrackItem,
    pub played_at: String,
    pub context: Option<SpotifyContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenData {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub created_at: i64,
}

impl TokenData {
    pub fn is_expired(&self) -> bool {
        chrono::Utc::now().timestamp() >= self.created_at + self.expires_in
    }
}

#[derive(Debug, Deserialize)]
pub struct UserProfile {
    pub id: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaybackEvent {
    pub id: i64,
    pub user_id: String,
    pub track_id: String,
    pub track_name: String,
    pub artist_name: String,
    pub album_name: String,
    pub duration_ms: i64,
    pub progress_ms: i64,
    pub is_playing: bool,
    pub shuffle: bool,
    pub repeat_state: String,
    pub context_uri: Option<String>,
    pub device_name: Option<String>,
    pub polled_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Classification {
    pub id: i64,
    pub user_id: String,
    pub track_id: String,
    pub track_name: String,
    pub artist_name: String,
    pub album_name: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub duration_ms: i64,
    pub listened_ms: i64,
    pub skipped: Option<bool>,
    pub context_uri: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Stats {
    pub total_listens: i64,
    pub completed: i64,
    pub skipped: i64,
    pub skip_rate: f64,
    pub top_skipped: Vec<TrackCount>,
    pub top_artists_by_skip_rate: Vec<ArtistSkipRate>,
}

#[derive(Debug, Serialize)]
pub struct TrackCount {
    pub track_name: String,
    pub artist_name: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct ArtistSkipRate {
    pub artist_name: String,
    pub total: i64,
    pub skipped: i64,
    pub skip_rate: f64,
}

impl TrackItem {
    pub fn artist_names(&self) -> String {
        self.artists
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_playback_state() {
        let json = r#"{
            "timestamp": 1774891915919,
            "device": {"id": "abc", "name": "My Phone", "type": "Smartphone"},
            "shuffle_state": true,
            "repeat_state": "off",
            "context": {"uri": "spotify:user:x:collection", "type": "collection"},
            "progress_ms": 18426,
            "item": {
                "id": "3U0NiT",
                "name": "Here I Am",
                "duration_ms": 239296,
                "artists": [{"id": "7Bg", "name": "LL Burns"}],
                "album": {"id": "5O9", "name": "Here I Am"},
                "uri": "spotify:track:3U0NiT"
            },
            "currently_playing_type": "track",
            "is_playing": true
        }"#;
        let state: PlaybackState = serde_json::from_str(json).unwrap();
        assert!(state.is_playing);
        assert_eq!(state.progress_ms, Some(18426));
        let item = state.item.unwrap();
        assert_eq!(item.name, "Here I Am");
        assert_eq!(item.artist_names(), "LL Burns");
    }

    #[test]
    fn deserialize_recently_played() {
        let json = r#"{
            "items": [{
                "track": {
                    "id": "3U0NiT",
                    "name": "Here I Am",
                    "duration_ms": 239296,
                    "artists": [{"id": "7Bg", "name": "LL Burns"}],
                    "album": {"id": "5O9", "name": "Here I Am"},
                    "uri": "spotify:track:3U0NiT"
                },
                "played_at": "2026-03-30T17:35:43.91Z",
                "context": null
            }]
        }"#;
        let resp: RecentlyPlayedResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.items.len(), 1);
        assert_eq!(resp.items[0].track.name, "Here I Am");
    }

    #[test]
    fn token_expiry() {
        let token = TokenData {
            access_token: "x".into(),
            token_type: "Bearer".into(),
            expires_in: 3600,
            refresh_token: None,
            scope: None,
            created_at: chrono::Utc::now().timestamp() - 7200,
        };
        assert!(token.is_expired());

        let fresh = TokenData {
            created_at: chrono::Utc::now().timestamp(),
            ..token
        };
        assert!(!fresh.is_expired());
    }
}
