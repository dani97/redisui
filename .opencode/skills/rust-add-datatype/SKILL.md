---
name: rust-add-datatype
description: Use when adding support for a new Redis data type to redisui (JSON, stream, bitmap, vector set, or changing how an existing type is read). Triggers on - add a data type, support JSON, new redis type, datatypes/, DataType, fetch_value.
---

# Adding a Redis data type

Five places. Missing any of the first two produces a type that silently never
appears in the UI, which is the failure mode to watch for.

## 1. `src/state.rs` — the variant

```rust
pub enum DataType {
    String, List, Hash, Set, SortedSet, Json,   // add here
}

pub const ALL: [DataType; 6] = [ /* every variant, in display order */ ];
```

Then its Redis type name and display label:

```rust
pub fn label(self) -> &'static str {
    DataType::Json => "json",
}

pub fn from_redis_type(name: &str) -> Option<Self> {
    "ReJSON-RL" | "ReJSON-RS" => Some(DataType::Json),
    _ => None,
}
```

- `ALL` is a fixed-size array, so adding a variant **will not compile** until
  you update it. That is the intended safety net, not an obstacle.
- `from_redis_type` must return `None` for types the app cannot render. Keys of
  those types are then skipped by the scan instead of crashing it. Match every
  module type name the server might report (`ReJSON-RL` and `ReJSON-RS` both
  exist depending on build).
- Also extend `redis_type()` if present, and `WIDTH` if the label is longer
  than the current `DataType::WIDTH`.

## 2. `src/ui/theme.rs` — a colour

```rust
DataType::Json => Color::Magenta,
```

## 3. `src/store/datatypes/mod.rs` — the dispatch arm

```rust
match data_type {
    DataType::Json => json::fetch(&mut conn, &key).await,
    _ => unreachable!(),
}
```

`_ =>` so the next addition is a compile error rather than a runtime surprise.

## 4. `src/store/datatypes/json.rs` — the module

Template, following `list.rs`:

```rust
use redis::RedisResult;
use crate::state::{ValueView, VALUE_ROW_LIMIT};
use crate::store::Conn;

pub(super) async fn fetch<C: Conn>(conn: &mut C, key: &str) -> RedisResult<ValueView> {
    let total: usize = redis::cmd("JSON.CARD").arg(key).query_async(conn).await?;
    let raw: String = redis::cmd("JSON.GET").arg(key).query_async(conn).await?;
    // ...build ValueView
}
```

Constraints the existing modules follow, keep them:

- `pub(super)`, and take `&mut C` where `C: Conn`.
- **Always bounded.** `VALUE_ROW_LIMIT` caps collection rows and
  `STRING_PREVIEW_BYTES` caps string previews. A module type with no natural
  cap (JSON documents) must be truncated explicitly and report the true total
  in `ValueView::Rows { total }` so the pane title can say so.
- Binary values go through `crate::store::types::decode`, which falls back to a
  hex preview. Never `String::from_utf8_lossy` directly.
- Reuse `headers(&[..])` from `datatypes/mod.rs` for column names.

## 5. `src/store/tests/datatypes.rs` — the test

```rust
#[tokio::test]
async fn fetches_json_rows() {
    let conn = FakeConn::new(vec![Ok(int(2)), array(vec![/* ... */])]);

    let (headers, rows, total) = rows_of(
        fetch_value_for(conn.clone(), "doc:1", DataType::Json).await.unwrap(),
    );

    assert_eq!(headers, vec!["path", "value"]);
    assert_eq!(total, 2);
    assert_eq!(
        conn.sent(),
        vec!["JSON.CARD doc:1".to_string(), "JSON.GET doc:1".to_string()],
        "the exact commands sent are part of the contract"
    );
}
```

The `conn.sent()` assertion is the point of the test: it pins the wire format,
which a unit test against hand-built replies otherwise cannot catch. Add a
matching case to `tests/live_store.rs` too — the fake cannot detect a command
the server actually rejects.

## Finally

- Update the `DataType` list in `README.md`.
- If the type needs a server module, probe for it at startup and grey the entry
  out when absent rather than failing to connect. `from_redis_type` returning
  `None` is the fallback that keeps the app usable on plain Redis.
- Run the gates: see the `rust-checks` skill.
