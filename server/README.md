# snowy-hub

The Snowy friends server. Friends connect through it, see each other online and pass small,
end-to-end encrypted messages. It also counts the anonymous daily usage pings.

## What it stores

- Public keys and the friend codes derived from them; friendships; pending requests
  (expire after 30 days). No names: nicknames live on each computer.
- Per-day totals: pings, pings per app version, friend-feature users, peak online.

No IP addresses or per-user activity are written to disk. The ping endpoint keeps a
per-address counter in memory for the current day only, to blunt casual inflation.

## Endpoints

| | |
|---|---|
| `GET /ws` | WebSocket protocol, see `src/proto.rs` |
| `POST /ping` | `{"ver":"0.1.6"}`, one per installation per day |
| `GET /stats` | daily totals, last 90 days |
| `GET /health` | `ok` |

## Run

    docker compose up -d --build

It listens on `127.0.0.1:8790` only; the Cloudflare tunnel publishes it as
`https://snowy.berkcantumer.dev`. Data is in `./data/snowy-hub.sqlite3`.

## Develop

    cargo test
