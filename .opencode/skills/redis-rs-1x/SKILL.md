---
name: redis-rs-1x
description: Use when changing anything under src/store, adding a Redis command, or debugging connection/auth/reply-parsing problems with the redis crate. Triggers on - redis, RedisResult, ConnectionManager, ConnectionInfo, ConnectionLike, query_async, pipeline, INFO, SCAN, auth, password, connection refused, NOAUTH, wrongtype.
---

# redis-rs 1.7 notes

Version-specific behaviour of the `redis` crate. Verify against the vendored
source in `~/.cargo/registry/src/*/redis-1.7.1/` when unsure — it is on disk.

## There are two `ConnectionLike` traits

This is the single most confusing thing in the crate:

- `redis::ConnectionLike` — **sync**
- `redis::aio::ConnectionLike` — **async**

`query_async` and `Pipeline::query_async` require the **aio** one. Passing
`MockRedisConnection` from `redis-test` fails, because it implements only the
sync trait. The error says `C implements similarly named trait
redis::ConnectionLike, but not redis::aio::ConnectionLike`, which reads like a
compiler bug and is not.

`src/store/tests/common.rs` therefore defines `FakeConn`, implementing the aio
trait directly.

## The `Conn` bound, and why it is not `Arc<dyn RedisRepository>`

```rust
pub trait Conn: ConnectionLike + Clone + Send + Sync + 'static {}
impl<T> Conn for T where T: ConnectionLike + Clone + Send + Sync + 'static {}
```

`RedisRepository` methods are **generic over `C: Conn`**, which makes the trait
non-`dyn`-safe. Two consequences, both deliberate:

- `event::dispatch` is generic over `R: RedisRepository` and uses
  `tokio::spawn`, not `Arc<dyn ...>`.
- The connection is passed **into** each command as an owned `C` rather than held
  by the repository. A `Session` cloned into a spawned task would otherwise keep
  its own copy of the cached connection and never see updates.

`ConnectionManager` is `Clone` and cheap (it wraps a multiplexed connection), so
owning a clone per command costs nothing.

## Reply parsing

- `query_async::<T>` is driven by `T: FromRedisValue`. Iterators return
  `RedisResult<Value>` (since v1), not `Value`.
- For shapes the crate does not model well (a `SCAN` reply, a `HSCAN` reply),
  query as `redis::Value` and decode with the helpers in
  `src/store/types.rs`: `cursor_reply`, `scan_pairs`, `scan_members`,
  `value_to_string`, `value_to_bytes`.
- `String: FromRedisValue` handles `BulkString`, `SimpleString`, `Okay`, `Int`
  and `Double`, so `TYPE` and `OBJECT ENCODING` decode into `String` fine.
  `Value` is `#[non_exhaustive]`, so matches need a `_` arm.
- Binary values: `STRLEN`/`GETRANGE` give `Vec<u8>`. Never
  `String::from_utf8_lossy` — use `types::decode`, which falls back to a hex
  preview and sets a `binary` flag for the UI.

## `ConnectionInfo` fields are private

```rust
let mut info = redis::IntoConnectionInfo::into_connection_info(url.as_str())?;
let settings = info.redis_settings().clone().set_db(3).set_password(pw);
info = info.set_redis_settings(settings);
let client = Client::open(info)?;
```

Getters (`redis_settings()`, `password()`, `username()`, `db()`) are public, so
tests can assert on credentials without a server:
`session.client.get_connection_info().redis_settings().password()`.

## Retries hide failures

`ConnectionManager` retries internally by default. With a wrong password that
means an **endless spinner and no error message**, which is the worst possible
UX for the most likely user mistake. So:

```rust
ConnectionManagerConfig::new()
    .set_connection_timeout(Some(CONNECT_TIMEOUT))
    .set_response_timeout(None)     // no client-side timeout; long SCANs are legit
    .set_number_of_retries(0);      // surface errors; we retry on a timer
```

Retrying is then the app's job, on a ~2s tick, which is also what lets the
status bar show `connect failed: ...` and then recover. Keep `number_of_retries`
at 0 unless you have a reason not to.

## Command shapes worth remembering

- Sub-commands are separate args: `cmd("OBJECT").arg("ENCODING").arg(key)`.
- `SCAN cursor [MATCH pattern] [COUNT count] [TYPE type]`. `TYPE` is Redis 6.0+
  and matches the `TYPE` command's name, but for module types the name varies
  (`ReJSON-RL` vs `ReJSON-RS`), so this app filters client-side with a pipelined
  `TYPE` instead. `MATCH` filters *after* fetching, so a selective pattern needs
  a much larger `COUNT` to return anything promptly.
- `ZRANGE key 0 N WITHSCORES` replies **member, score, member, score** — the
  score trails its member. Getting this backwards silently swaps two columns.
- Pipelines return a collection even for one command.
- `MEMORY USAGE` returns nil for a key with no key memory, so decode as
  `Option<u64>`.
- `TTL` returns `-1` for no expiry and `-2` for a missing key; both are
  meaningful and must be distinguished.

## Blocking

Never issue `KEYS`, `SMEMBERS`, `HGETALL` or an unbounded `LRANGE` against a
key that might be large. `KEYS` is O(N) and blocks the server. This app reads
every collection through a cursor or a capped range, and the caps live in
`src/state.rs` (`VALUE_ROW_LIMIT`, `KEYS_PER_TYPE_LIMIT`,
`STRING_PREVIEW_BYTES`) so the UI can report when one is hit.
