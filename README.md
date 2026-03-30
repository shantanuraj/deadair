# deadair

Spotify playback tracker. Polls the Spotify API every 5 seconds, records what
you listen to, and classifies each track as skipped or played through.

## Setup

Requires a [Spotify developer app](https://developer.spotify.com/dashboard).

```
cp .env.sample .env
# fill in SPOTIFY_CLIENT_ID, SPOTIFY_CLIENT_SECRET, DEADAIR_SECRET
```

Set your redirect URI in the Spotify dashboard:

- dev: `http://127.0.0.1:8080/callback`
- prod: `https://your-domain.com/callback`

For production, set `DEADAIR_HOST=https://your-domain.com` in `.env`.

## Run

```
cargo run
```

Visit `http://127.0.0.1:8080`, log in with Spotify. Polling starts immediately.

## How it works

Three background tasks run per authenticated user:

| Task            | Interval | Endpoint                         |
| --------------- | -------- | -------------------------------- |
| Playback poller | 5s       | `GET /me/player`                 |
| Reconciler      | 60s      | `GET /me/player/recently-played` |
| Token refresh   | 20m      | `POST /api/token`                |

The poller records a snapshot every 5 seconds. When the track changes, it closes
out the previous listen and computes:

```
skipped = listened_ms < duration_ms * 0.8
```

The reconciler backfills any gaps from poller downtime.

## Export

```
GET /api/events?format=json&last=7d
GET /api/events?format=csv&since=2026-03-30&until=2026-04-13
GET /api/stats
```

## Schema

Three tables in a single SQLite file (`deadair.db`):

**users** -- authenticated Spotify accounts and their tokens.

**playback_events** -- raw 5-second polling snapshots: track, progress,
shuffle/repeat state, device, context.

**classifications** -- one row per listen: track, how long it played, whether it
was skipped, what playlist/album it came from.

## Configuration

| Variable                | Required | Default                   |
| ----------------------- | -------- | ------------------------- |
| `SPOTIFY_CLIENT_ID`     | yes      |                           |
| `SPOTIFY_CLIENT_SECRET` | yes      |                           |
| `DEADAIR_SECRET`        | yes      |                           |
| `DEADAIR_DB`            | no       | `deadair.db`              |
| `DEADAIR_HOST`          | no       | `http://127.0.0.1:{PORT}` |
| `DEADAIR_RECONCILE`     | no       | `true`                    |
| `PORT`                  | no       | `8080`                    |

## Deploy

```
cargo build --release
```

Systemd unit included at `deadair.service`. Symlink it, enable, start:

```
sudo ln -sf $(pwd)/deadair.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now deadair
```

## License

MIT
