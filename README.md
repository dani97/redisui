# redisui

A read-only terminal UI browser for Redis, built with [ratatui](https://ratatui.rs) and crossterm.

```
┌▸ types─────────────────┐┌▸ keys  hashes  2/2──────────────────────────────────────────────────────────┐
│strings     4           ││user:1                                                                         │
│arrays      1           ││user:2                                                                         │
│hashes      2           ││                                                                              │
│sets        1           ││                                                                              │
│sorted sets 1           ││                                                                              │
└────────────────────────┘└──────────────────────────────────────────────────────────────────────────┘
┌  detail  user:1────────┐┌▸ value  user:1  3 rows──────────────────────────────────────────────────────┐
│type      hashes        ││field value                                                                     │
│ttl       no expiry     ││name  alice                                                                     │
│encoding  listpack      ││email a@b.c                                                                     │
│memory    96 B          ││age   31                                                                        │
└────────────────────────┘└──────────────────────────────────────────────────────────────────────────┘
● connected redis://127.0.0.1:6390 v7.4.11                        tab:panes /:filter r:refresh ?:help q:quit
```

Data types supported: **string**, **array (list)**, **hash**, **set**, **sorted set**.
Keyspace discovery uses a single `SCAN` pass with a pipelined `TYPE` per batch, so all five
type counts come out of one traversal.

---

## Requirements

- Rust 1.98+ (the crate is edition 2024)
- A reachable Redis server (any 6.0+; a throwaway one is easy — see below)

## Quick start

```bash
# 1. start a throwaway Redis (skip if you already have one)
docker run -d --name redisui-test -p 6390:6379 redis:7-alpine

# 2. run the app against it
cargo run -- --url redis://127.0.0.1:6390
```

Tear the container down with `docker rm -f redisui-test`.

### Connection options

| Source | Example |
| --- | --- |
| default | `cargo run` → `redis://127.0.0.1:6379` |
| flag | `cargo run -- --url redis://127.0.0.1:6390` |
| env | `REDIS_URL=redis://host:6390 cargo run` |
| database | `cargo run -- --url redis://127.0.0.1:6390 --db 3` |

`--url` takes precedence over `REDIS_URL`, which takes precedence over the default.
`--db` is only applied when you pass it, so a database index in the URL is otherwise respected.

### Authentication

Passwords work four ways. In precedence order, first match wins:

| How | Example |
| --- | --- |
| `--password-file` | `cargo run -- --password-file /run/secrets/redis` |
| `--password` | `cargo run -- --password hunter2` |
| `REDIS_PASSWORD` | `REDIS_PASSWORD=hunter2 cargo run` |
| in the URL | `cargo run -- --url 'redis://:hunter2@host:6379'` |

Add `-a/--username` (or `REDIS_USERNAME`) for ACL users. Flags and env vars override
credentials embedded in the URL, since they are the more local signal.

```bash
# password-only (legacy AUTH), embedded in the URL
cargo run -- --url 'redis://:hunter2@host:6379'

# ACL user, credentials kept out of the URL entirely
REDIS_URL=redis://host:6379 REDIS_USERNAME=app REDIS_PASSWORD=hunter2 cargo run

# from a file, the safest option
cargo run -- --url redis://host:6379 --password-file /run/secrets/redis
```

Two things to know:

- **A password in the URL must be percent-encoded.** `@`, `:` and `/` are URL
  structure, so encode them: `@` → `%40`, `:` → `%3A`, `/` → `%2F`. A raw `@` in
  the password is parsed as the host delimiter and the connection will fail
  confusingly. `--password` / `--password-file` skip this entirely.
- **`--password` is visible in `ps` and shell history.** Prefer
  `--password-file` or `REDIS_PASSWORD`. A password file may end with a newline
  (`echo secret > f` is fine); only that trailing newline is stripped, so
  passwords with leading or trailing spaces still work. An empty file is
  rejected rather than sent as an empty password.

Whatever the source, **the password is never displayed**: the status bar shows
`redis://app:***@host:6379`, and `--help` will not echo an env-provided value.

TLS (`rediss://`) is not supported — the `tls-rustls` feature is not enabled, so
such a URL fails with `can't connect with TLS, the feature is not enabled`
rather than silently falling back to plaintext.

---

## Seeding demo data

This gives you one of every supported type, plus a key that expires on its own and one that
holds raw non-UTF-8 bytes. Run it with the container up:

```bash
docker exec redisui-test redis-cli FLUSHALL >/dev/null

# strings
docker exec redisui-test redis-cli SET greeting "hello redisui" >/dev/null
docker exec redisui-test redis-cli SET counter:1 "1000" >/dev/null
docker exec redisui-test redis-cli SETEX session:abc 300 "temp-token" >/dev/null
printf '\xff\xfe\x00\x01\x41' | docker exec -i redisui-test redis-cli -x SET bin:raw >/dev/null

# array (Redis list)
docker exec redisui-test redis-cli RPUSH queue:jobs a b c d e >/dev/null

# hashes
docker exec redisui-test redis-cli HSET user:1 name alice email a@b.c age 31 >/dev/null
docker exec redisui-test redis-cli HSET user:2 name bob email b@c.d >/dev/null

# set
docker exec redisui-test redis-cli SADD tags red green blue >/dev/null

# sorted set — note ZADD takes score *then* member
docker exec redisui-test redis-cli ZADD leaderboard 1500 alice 2100 bob 900 carol >/dev/null
```

Verify from the CLI that the app's view should match:

```bash
docker exec redisui-test redis-cli dbsize                       # 9
docker exec redisui-test redis-cli type leaderboard            # zset
docker exec redisui-test redis-cli --no-raw zrange leaderboard 0 -1 WITHSCORES
# 1) "carol"  2) "900"  3) "alice"  4) "1500"  5) "bob"  6) "2100"
```

## Keybindings

| Key | Action |
| --- | --- |
| `tab` / `shift+tab` | next / previous pane |
| `↑` `↓` or `k` `j` | move selection |
| `page up` / `page down` | move by ten |
| `home` / `end` or `g` / `G` | jump to first / last |
| `enter` | types → keys → value |
| `/` | filter keys by glob pattern |
| `r` | rescan the keyspace |
| `?` | toggle help |
| `q` / `ctrl+c` | quit |

While the filter is open, every printable key types into it (so `j`, `r`, `q` and `/` are
literal text, not commands); `enter` applies and `esc` cancels.

---

## Manual test checklist

Start the app and the seeded data, then walk down this list. Each row is a behaviour that is
easy to break, so it is worth re-running after touching the store or reducer.

| # | Do this | Expect |
| --- | --- | --- |
| 1 | launch | status bar reads `● connected … v7.4.11`; types pane shows `strings 4, arrays 1, hashes 2, sets 1, sorted sets 1`; keys pane title shows `4/4` for strings |
| 2 | `j` ×4 through the types pane | each type's key list loads as you land on it; the focused pane border turns cyan and its title gains a `▸` |
| 3 | stop on **hashes** | keys pane lists `user:1`, `user:2`; the detail pane fills with type/ttl/encoding/memory |
| 4 | stop on **sorted sets** | value pane shows `score member` with `900 carol`, `1500 alice`, `2100 bob` — ascending, correctly paired |
| 5 | stop on **arrays** | value pane shows an indexed list `0 a` … `4 e` |
| 6 | stop on **sets** | value pane lists `red`, `green`, `blue` under a `member` header |
| 7 | select `bin:raw` | title says `5 bytes, binary`; body shows the hex fallback `fffe000141`, not mojibake |
| 8 | select `session:abc` | detail pane `ttl` shows a non-zero countdown; after ~5 minutes the key is gone and the count drops to 3 strings |
| 9 | `/` then `user:*` then `enter` | only `user:1`, `user:2` remain; status bar right side reads `2 of 9 match`; the other types show `0` |
| 10 | `/` then `esc` | filter is discarded, previous key list returns |
| 11 | `r` | keyspace rescans; counts return to the unfiltered totals |
| 12 | `tab` to the value pane, `j` `j` | selection moves through rows independently of the key list |
| 13 | `?` | help overlay; any key dismisses it and does **not** leak through to the app |
| 14 | shrink the terminal to ~40×12, then ~200×50 | no panic, panes re-flow, long values elide with `…` |
| 15 | `CONFIG SET requirepass hunter2` on the server, then connect with `--password hunter2` | `● connected`, and the status bar shows no password at all |
| 16 | repeat with `--password-file` and with a wrong password | correct file connects; a wrong one reports `connect failed: Password authentication failed` within a couple of seconds rather than spinning |
| 17 | connect with `--url 'redis://:hunter2@host:6379'` | connects, and the status bar reads `redis://:***@host:6379` — never the secret |
| 18 | stop the container | within ~2s the status flips to `● offline` and the right side reads `connection lost: broken pipe`; start the container again and it reconnects and rescans by itself, no restart needed |
| 19 | `q` | exits 0 and restores your terminal (no leftover alternate-screen state) |

Checks 15-17 cover authentication; 18 is the important one for resilience. A `PING` heartbeat runs every ~2 seconds, so
the app notices a server that went away even while you are not touching it — which it would
otherwise have no way to find out, since an idle UI never talks to the socket.

### Testing without Docker

Point `--url` at any local Redis, or run one without a container:

```bash
redis-server --daemonize yes --port 6390 --save '' --appendonly no
```

---

## Automated tests

```bash
cargo test                  # unit + render tests, no Redis required
cargo clippy --all-targets  # expect zero warnings
cargo fmt --check           # expect no diff
```

### Integration tests (live Redis)

`tests/live_store.rs` drives the real `redis` crate against a real server through
`Session`. It is opt-in via an environment variable so `cargo test` can never
touch a database you care about:

```bash
docker run -d --name redisui-test -p 6390:6379 redis:7-alpine
REDISUI_TEST_REDIS=redis://127.0.0.1:6390/15 cargo test --test live_store
```

Without the variable every test prints why it was skipped and passes. Use a
throwaway database index (e.g. `/15`): tests isolate themselves with a per-test
key prefix rather than `FLUSHDB`, so they are parallel-safe and leave the rest of
the database alone.

19 scenarios, including: version reporting, all five value types read back
correctly, binary strings decoded as hex, sorted-set score/member pairing,
row capping on an oversized list, TTL counting down, an expired key reporting
that it vanished, a stream being excluded as undisplayable, glob filtering,
multi-page cursor resumption losing no keys, and one database never seeing
another's keys.

To also exercise authentication, set a throwaway password — the test sets
`requirepass`, checks that a session with the password works and one without is
rejected, then clears it again:

```bash
REDISUI_TEST_REDIS=redis://127.0.0.1:6390/15 \
REDISUI_TEST_PASSWORD=hunter2 cargo test --test live_store authenticates
```

`cargo test` needs no server and no network: the store layer is driven by a `FakeConn` that
implements redis-rs's async `ConnectionLike`, replays canned `Value` replies **and records the
command bytes**, so the tests assert the wire format (`STRLEN greeting`,
`GETRANGE greeting 0 8191`, `ZRANGE board 0 999 WITHSCORES`, …) as well as the parsing.

Coverage by area:

| Area | What is covered |
| --- | --- |
| `store::tests::value` | every per-type fetch, binary/hex fallback, exact commands sent, error and empty-key cases |
| `store::tests::scan` | keyspace scan: type bucketing, filters, cursor resumption, pipeline contents, unsupported types |
| `store::tests::detail` | TTL variants, encoding, memory, vanished and unsupported keys |
| `store::session` | credential resolution, password files, URL redaction |
| `store::types` | RESP decoding: `SCAN` cursors, `HSCAN` pairs, `SSCAN` members, status/int replies |
| `app` (reducer) | focus cycling, boundary clamping, per-type key bucketing, stale-result rejection by request id, filter editing, connection retry, heartbeat cadence and recovery, help overlay |
| `ui::text` | width-aware padding and elision, including multibyte input |
| `ui::layout` | pane geometry |
| `ui` (render) | full-frame renders through ratatui's `TestBackend`, asserted as text, including 1×1 and 200×60 terminals |

The reducer is a pure function, `app::reduce(&mut state, event) -> Vec<Command>`, so it is
tested without spawning tasks or touching a socket. I/O is a separate step (`event::dispatch`),
which is what keeps the render loop from ever blocking on the network.

### Adding a data type

1. Add the variant to `DataType` and `DataType::ALL`, map its `TYPE` string in
   `from_redis_type`, and give it a colour in `ui::theme`.
2. Add an arm to `fetch_value` in `store/mod.rs` producing a `ValueView`, and a test against
   `FakeConn` that asserts both the reply parsing and the command sent.
3. Add a `FakeConn` test — this is the part that catches wire-format mistakes.

JSON is deliberately absent: it needs the RedisJSON module, and `from_redis_type` returns
`None` for module types so such keys are simply left out of the type list. When you add it,
probe for the module at startup and grey the entry out on plain Redis.

---

## Troubleshooting

| Symptom | Cause |
| --- | --- |
| `● offline` in the status bar | nothing listening on that host/port. The app keeps probing every ~2s; fix the server and it reconnects and rescans by itself. |
| `connection lost: …` on the right of the status bar | the link dropped mid-session. Same recovery as above; no restart needed. |
| Everything shows `0` | connected to an empty database, or you passed `--db N` for a database you did not seed. |
| `connect failed: …` | bad URL, auth required, or TLS. The full message is on the status bar. |
| `connect failed: "NOAUTH"` | the server needs a password and none was supplied. See [Authentication](#authentication). |
| `Password authentication failed` | the password is wrong. If it came from a URL, check it is percent-encoded. |
| `can't connect with TLS, the feature is not enabled` | you used `rediss://`; TLS is not supported yet. |
| Terminal looks garbled after quitting | shouldn't happen — `ratatui::init` installs a panic hook that restores the screen. If you see this, it's worth a bug report. |
| `ZADD` fails with `value is not a valid float` | the argument order is `ZADD key score member …`, not `member score`. |

## Layout

```
src/
  main.rs        CLI -> tokio runtime -> ratatui::init -> app::run -> restore
  cli.rs         clap args, env fallback
  app.rs         run loop + the pure reducer
  event.rs       crossterm stream, tick, action translation, worker dispatch
  command.rs     outbound effects
  state.rs       AppState, DataType, ValueView, per-pane ListStates
  store/         the only code that talks to redis
    mod.rs       Conn/RedisRepository traits, ScanPage
    session.rs   Client/ConnectionManager construction, credentials, redaction
    scan.rs      SCAN + pipelined TYPE
    detail.rs    TYPE / TTL / OBJECT ENCODING / MEMORY USAGE
    types.rs     RESP decoding and binary-safe text
    value/       one module per datatype
      mod.rs     the single fetch_value dispatch point
      string.rs  list.rs  hash.rs  set.rs  zset.rs
    tests/       unit tests, driven by a fake async connection
  ui/
    mod.rs       draw() -> pane rects -> panes
    layout.rs    geometry
    theme.rs     colours, focus marks
    text.rs      width-aware pad/elide
    help.rs      help overlay
    panes/       types, keys, value, detail, status
```

Design notes:

- **Unidirectional flow.** `AppEvent -> reduce() -> Command -> dispatch -> AppEvent`, and
  `state -> draw()`. State is the single source of truth; the reducer never performs I/O.
- **Connection is threaded, not hidden.** `Command` carries a cheap `ConnectionManager` clone,
  so a worker never holds a stale copy of the session. A single failure never takes down the
  event loop.
- **Liveness is polled, not assumed.** A `PING` heartbeat every ~2s drives the connected/offline
  indicator. On failure the cached connection is dropped and a fresh one is built on a ~2s
  timer, which both surfaces errors promptly (a wrong password used to spin forever) and
  recovers without a restart.
- **Adding a datatype touches four places:** a `DataType` variant with its `TYPE` mapping, an
  arm in `value/mod.rs`, a module in `value/`, and a test in `store/tests/value.rs`.
- **Stale results are dropped by request id.** Every value/detail load carries an id; a reply
  for a key you have since moved off is discarded rather than rendered.
- **Bounded reads.** Keys are capped at 2,000 per type, collection rows at 1,000, and string
  previews at 8 KB, so a multi-million-key database cannot stall or exhaust the UI. Pane
  titles say when a cap was hit.
