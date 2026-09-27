//! Integration tests: the real `redis` crate against a real server, exercising
//! the `Session` connection path and every `RedisRepository` query.
//!
//! These are opt-in so that `cargo test` never touches a database you care
//! about. Point them at a throwaway server:
//!
//! ```text
//! docker run -d --name redisui-test -p 6390:6379 redis:7-alpine
//! REDISUI_TEST_REDIS=redis://127.0.0.1:6390/15 cargo test --test live_store
//! ```
//!
//! Without the variable every test reports that it was skipped and passes.
//! Tests isolate themselves with a key prefix rather than `FLUSHDB`, so they are
//! safe to run in parallel and leave the rest of the database alone.

use std::sync::atomic::{AtomicU32, Ordering};

use redis::AsyncCommands;
use redis::aio::ConnectionManager;

use redisui::state::{DataType, Ttl, VALUE_ROW_LIMIT, ValueView};
use redisui::store::{RedisRepository, Session};

const ENV: &str = "REDISUI_TEST_REDIS";

/// One test server plus a key prefix nothing else uses.
struct Live {
    session: Session,
    conn: ConnectionManager,
    prefix: String,
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// `None` means "not configured, so skip".
///
/// Once `{ENV}` is set the server is *expected* to be there, so an unreachable
/// one is a failure rather than a skip. Skipping in that case produces a
/// convincing all-green run in which nothing was actually verified, which is
/// worse than a red build.
fn configured_url() -> Option<String> {
    match std::env::var(ENV) {
        Ok(url) if url.trim().is_empty() => None,
        Ok(url) => Some(url),
        Err(_) => None,
    }
}

impl Live {
    /// `None` means "skipped": no server configured.
    async fn new(tag: &str) -> Option<Self> {
        let Some(url) = configured_url() else {
            eprintln!("skipped: set {ENV} to run the live store tests");
            return None;
        };

        let session = Session::new(&args_for(&url))
            .unwrap_or_else(|error| panic!("{ENV} is set but unusable: {url}: {error}"));
        let conn = session
            .connect()
            .await
            .unwrap_or_else(|error| panic!("{ENV} is set but unreachable: {url}: {error}"));

        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        Some(Self {
            session,
            conn,
            prefix: format!("redisui-it:{tag}:{unique}:"),
        })
    }

    fn conn(&self) -> ConnectionManager {
        self.conn.clone()
    }

    fn key(&self, suffix: &str) -> String {
        format!("{}{suffix}", self.prefix)
    }

    async fn scan<C: redisui::store::Conn>(
        &self,
        conn: C,
        cursor: u64,
        filter: &str,
        count: usize,
    ) -> redis::RedisResult<redisui::store::ScanPage> {
        self.session.scan(conn, cursor, filter, count).await
    }

    async fn fetch_value<C: redisui::store::Conn>(
        &self,
        conn: C,
        key: &str,
        data_type: DataType,
    ) -> redis::RedisResult<ValueView> {
        self.session.fetch_value(conn, key, data_type).await
    }

    async fn fetch_detail<C: redisui::store::Conn>(
        &self,
        conn: C,
        key: &str,
    ) -> redis::RedisResult<redisui::state::KeyDetail> {
        self.session.fetch_detail(conn, key).await
    }

    async fn ping<C: redisui::store::Conn>(&self, conn: C) -> redis::RedisResult<()> {
        self.session.ping(conn).await
    }

    async fn clear(&self) {
        let pattern = format!("{}*", self.prefix);
        let mut cursor = 0u64;
        loop {
            let (next, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&pattern)
                .arg("COUNT")
                .arg(500)
                .query_async(&mut self.conn.clone())
                .await
                .expect("scan");
            cursor = next;
            if !keys.is_empty() {
                let mut conn = self.conn.clone();
                let _: usize = conn.del(keys).await.expect("del");
            }
            if cursor == 0 {
                break;
            }
        }
    }

    /// Scans the whole keyspace the way the app does, returning what was found.
    async fn scan_all(&self, filter: &str, count: usize) -> Vec<(DataType, String)> {
        let mut found = Vec::new();
        let mut cursor = 0u64;
        loop {
            let page = self
                .scan(self.conn.clone(), cursor, filter, count)
                .await
                .expect("scan page");
            found.extend(page.keys);
            cursor = page.cursor;
            if page.done {
                return found;
            }
        }
    }
}

fn args_for(url: &str) -> redisui::cli::Args {
    use clap::Parser;
    redisui::cli::Args::parse_from(["redisui", "--url", url])
}

/// Same server, different database index. Assumes a plain `redis://host:port`
/// URL with no query string, which is all the test harness needs.
fn url_for_db(url: &str, db: u32) -> String {
    match url.split_once("://") {
        Some((scheme, rest)) => {
            let authority = rest.split('/').next().unwrap_or(rest);
            format!("{scheme}://{authority}/{db}")
        }
        None => format!("{url}/{db}"),
    }
}

fn rows_of(view: ValueView) -> (Vec<String>, Vec<Vec<String>>, usize) {
    match view {
        ValueView::Rows {
            headers,
            rows,
            total,
        } => (headers, rows, total),
        other => panic!("expected rows, got {other:?}"),
    }
}

#[tokio::test]
async fn connect_reports_a_redis_version() {
    let Some(live) = Live::new("connect").await else {
        return;
    };
    let session = Session::new(&args_for(
        &configured_url().expect("env checked by Live::new"),
    ))
    .unwrap();
    let (_, info) = RedisRepository::connect(&session).await.expect("connect");

    assert_ne!(info.version, "unknown", "INFO should yield a version");
    assert!(info.version.starts_with(|c: char| c.is_ascii_digit()));
    drop(live);
}

#[tokio::test]
async fn ping_reports_a_live_link() {
    let Some(live) = Live::new("ping").await else {
        return;
    };
    live.ping(live.conn()).await.expect("ping");
}

#[tokio::test]
async fn scan_indexes_every_supported_type() {
    let Some(live) = Live::new("scan-all").await else {
        return;
    };
    live.clear().await;

    let string = live.key("s");
    let list = live.key("l");
    let hash = live.key("h");
    let set = live.key("t");
    let zset = live.key("z");
    let _: () = live.conn().set(&string, "v").await.unwrap();
    let _: () = live.conn().rpush(&list, &["a", "b"]).await.unwrap();
    let _: () = live.conn().hset(&hash, "f", "v").await.unwrap();
    let _: () = live.conn().sadd(&set, "m").await.unwrap();
    let _: () = live.conn().zadd(&zset, "m", 1).await.unwrap();

    let found = live.scan_all("", 500).await;
    let types: Vec<DataType> = found.iter().map(|(dt, _)| *dt).collect();
    for expected in [
        DataType::String,
        DataType::List,
        DataType::Hash,
        DataType::Set,
        DataType::SortedSet,
    ] {
        assert!(
            types.contains(&expected),
            "missing {expected:?} in {types:?}"
        );
    }
    assert!(found.iter().any(|(_, k)| k == &string));
    assert!(found.iter().any(|(_, k)| k == &zset));
    live.clear().await;
}

#[tokio::test]
async fn scan_skips_types_it_cannot_display() {
    let Some(live) = Live::new("scan-skip").await else {
        return;
    };
    live.clear().await;

    let visible = live.key("plain");
    let _: () = live.conn().set(&visible, "v").await.unwrap();

    // A stream is a real Redis type the app does not render.
    let stream = live.key("stream");
    let _: String = redis::cmd("XADD")
        .arg(&stream)
        .arg("*")
        .arg("f")
        .arg("v")
        .query_async(&mut live.conn())
        .await
        .expect("xadd");

    let found = live.scan_all(&format!("{}*", live.prefix), 500).await;
    assert_eq!(
        found,
        vec![(DataType::String, visible)],
        "the string is indexed and the stream is dropped"
    );
    live.clear().await;
}

#[tokio::test]
async fn scan_honours_a_glob_filter() {
    let Some(live) = Live::new("scan-filter").await else {
        return;
    };
    live.clear().await;

    let wanted = live.key("wanted");
    let ignored = live.key("other");
    let _: () = live.conn().set(&wanted, "1").await.unwrap();
    let _: () = live.conn().set(&ignored, "2").await.unwrap();

    let found = live.scan_all(&format!("{}wanted*", live.prefix), 500).await;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].1, wanted);
    live.clear().await;
}

#[tokio::test]
async fn scan_resumes_across_pages_without_losing_keys() {
    let Some(live) = Live::new("scan-pages").await else {
        return;
    };
    live.clear().await;

    let count = 25;
    for index in 0..count {
        let _: () = live
            .conn()
            .set(live.key(&format!("k{index:02}")), "x")
            .await
            .unwrap();
    }

    // COUNT 2 forces many pages, exercising cursor threading.
    let mut pages = 0;
    let mut seen: Vec<String> = Vec::new();
    let mut cursor = 0u64;
    loop {
        let page = live
            .scan(live.conn(), cursor, &format!("{}*", live.prefix), 2)
            .await
            .expect("scan page");
        pages += 1;
        seen.extend(page.keys.into_iter().map(|(_, key)| key));
        cursor = page.cursor;
        if page.done {
            break;
        }
        assert!(pages < 200, "cursor failed to terminate");
    }

    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), count, "every key should appear exactly once");
    assert!(pages > 1, "COUNT 2 should have needed several pages");
    live.clear().await;
}

#[tokio::test]
async fn reads_a_string_value() {
    let Some(live) = Live::new("v-string").await else {
        return;
    };
    let key = live.key("greeting");
    let _: () = live.conn().set(&key, "hello redisui").await.unwrap();

    let view = live
        .fetch_value(live.conn(), &key, DataType::String)
        .await
        .unwrap();

    match view {
        ValueView::Scalar {
            text,
            bytes,
            binary,
        } => {
            assert_eq!(text, "hello redisui");
            assert_eq!(bytes, 13);
            assert!(!binary);
        }
        other => panic!("expected scalar, got {other:?}"),
    }
    live.clear().await;
}

#[tokio::test]
async fn reads_a_binary_string_as_hex() {
    let Some(live) = Live::new("v-binary").await else {
        return;
    };
    let key = live.key("blob");
    let _: () = live
        .conn()
        .set(&key, vec![0xff_u8, 0x00, 0x41])
        .await
        .unwrap();

    let view = live
        .fetch_value(live.conn(), &key, DataType::String)
        .await
        .unwrap();

    match view {
        ValueView::Scalar { text, binary, .. } => {
            assert!(binary, "invalid utf-8 must be flagged");
            assert_eq!(text, "ff0041");
        }
        other => panic!("expected scalar, got {other:?}"),
    }
    live.clear().await;
}

#[tokio::test]
async fn reads_a_list_with_indices() {
    let Some(live) = Live::new("v-list").await else {
        return;
    };
    let key = live.key("queue");
    let _: () = live.conn().rpush(&key, &["a", "b", "c"]).await.unwrap();

    let (headers, rows, total) = rows_of(
        live.fetch_value(live.conn(), &key, DataType::List)
            .await
            .unwrap(),
    );

    assert_eq!(headers, vec!["#", "datatypes"]);
    assert_eq!(rows, vec![vec!["0", "a"], vec!["1", "b"], vec!["2", "c"]]);
    assert_eq!(total, 3);
    live.clear().await;
}

#[tokio::test]
async fn reads_a_hash_as_field_value_pairs() {
    let Some(live) = Live::new("v-hash").await else {
        return;
    };
    let key = live.key("user");
    let _: () = live.conn().hset(&key, "name", "alice").await.unwrap();
    let _: () = live.conn().hset(&key, "age", "31").await.unwrap();

    let (headers, rows, total) = rows_of(
        live.fetch_value(live.conn(), &key, DataType::Hash)
            .await
            .unwrap(),
    );

    assert_eq!(headers, vec!["field", "datatypes"]);
    assert_eq!(total, 2);
    let mut pairs: Vec<(String, String)> = rows
        .into_iter()
        .map(|row| (row[0].clone(), row[1].clone()))
        .collect();
    pairs.sort();
    assert_eq!(
        pairs,
        vec![
            ("age".to_string(), "31".to_string()),
            ("name".to_string(), "alice".to_string())
        ]
    );
    live.clear().await;
}

#[tokio::test]
async fn reads_a_set_of_members() {
    let Some(live) = Live::new("v-set").await else {
        return;
    };
    let key = live.key("tags");
    let _: () = live.conn().sadd(&key, &["red", "green"]).await.unwrap();

    let (headers, mut rows, total) = rows_of(
        live.fetch_value(live.conn(), &key, DataType::Set)
            .await
            .unwrap(),
    );

    assert_eq!(headers, vec!["member"]);
    assert_eq!(total, 2);
    rows.sort();
    assert_eq!(rows, vec![vec!["green"], vec!["red"]]);
    live.clear().await;
}

#[tokio::test]
async fn reads_a_sorted_set_as_score_member_pairs() {
    let Some(live) = Live::new("v-zset").await else {
        return;
    };
    let key = live.key("board");
    let _: () = live.conn().zadd(&key, "alice", 1500).await.unwrap();
    let _: () = live.conn().zadd(&key, "bob", 2100).await.unwrap();
    let _: () = live.conn().zadd(&key, "carol", 900).await.unwrap();

    let (headers, rows, total) = rows_of(
        live.fetch_value(live.conn(), &key, DataType::SortedSet)
            .await
            .unwrap(),
    );

    assert_eq!(headers, vec!["score", "member"]);
    assert_eq!(total, 3);
    // Ascending by score, each score attached to the right member.
    assert_eq!(
        rows,
        vec![
            vec!["900", "carol"],
            vec!["1500", "alice"],
            vec!["2100", "bob"],
        ]
    );
    live.clear().await;
}

#[tokio::test]
async fn caps_long_collections_and_reports_the_true_total() {
    let Some(live) = Live::new("v-cap").await else {
        return;
    };
    let key = live.key("big");
    let total = VALUE_ROW_LIMIT + 1;
    let members: Vec<String> = (0..total).map(|index| format!("m{index}")).collect();
    let _: () = live.conn().rpush(&key, &members).await.unwrap();

    let (_, rows, reported) = rows_of(
        live.fetch_value(live.conn(), &key, DataType::List)
            .await
            .unwrap(),
    );

    assert_eq!(rows.len(), VALUE_ROW_LIMIT, "rows are capped");
    assert_eq!(reported, total, "the true length is still reported");
    live.clear().await;
}

#[tokio::test]
async fn detail_reports_persistent_keys() {
    let Some(live) = Live::new("d-perm").await else {
        return;
    };
    let key = live.key("perm");
    let _: () = live.conn().set(&key, "v").await.unwrap();

    let detail = live.fetch_detail(live.conn(), &key).await.unwrap();

    assert_eq!(detail.data_type, DataType::String);
    assert_eq!(detail.ttl, Ttl::Persistent);
    assert!(!detail.encoding.is_empty());
    assert!(detail.memory_bytes.unwrap() > 0);
    live.clear().await;
}

#[tokio::test]
async fn detail_reports_a_counting_down_ttl() {
    let Some(live) = Live::new("d-ttl").await else {
        return;
    };
    let key = live.key("temp");
    let _: () = live.conn().set_ex(&key, "v", 120).await.unwrap();

    let detail = live.fetch_detail(live.conn(), &key).await.unwrap();

    match detail.ttl {
        Ttl::Expiring(seconds) => {
            assert!(seconds > 0 && seconds <= 120, "unexpected ttl {seconds}");
        }
        other => panic!("expected an expiring ttl, got {other:?}"),
    }
    live.clear().await;
}

#[tokio::test]
async fn detail_reports_a_vanished_key() {
    let Some(live) = Live::new("d-gone").await else {
        return;
    };
    let key = live.key("doomed");
    let _: () = live.conn().set_ex(&key, "v", 1).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

    let error = live
        .fetch_detail(live.conn(), &key)
        .await
        .expect_err("the key has expired");

    assert!(
        error.to_string().contains("key no longer exists"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn detail_rejects_types_the_app_cannot_render() {
    let Some(live) = Live::new("d-unsupported").await else {
        return;
    };
    let key = live.key("stream");
    let _: String = redis::cmd("XADD")
        .arg(&key)
        .arg("*")
        .arg("f")
        .arg("v")
        .query_async(&mut live.conn())
        .await
        .expect("xadd");

    let error = live
        .fetch_detail(live.conn(), &key)
        .await
        .expect_err("streams are not displayable");

    assert!(
        error.to_string().contains("unsupported key type"),
        "unexpected error: {error}"
    );
    live.clear().await;
}

#[tokio::test]
async fn only_the_selected_database_is_visible() {
    let Some(live) = Live::new("db-iso").await else {
        return;
    };
    let url = configured_url().expect("env checked by Live::new");

    // Write into database 0 using a separate session.
    let other = Session::new(&args_for(&url_for_db(&url, 0))).unwrap();
    let other_conn = other.connect().await.expect("connect to db 0");
    let mut other_conn_mut = other_conn.clone();
    let marker = format!("redisui-it:iso:{}", COUNTER.fetch_add(1, Ordering::Relaxed));
    let _: () = other_conn_mut.set(&marker, "elsewhere").await.unwrap();

    live.clear().await;
    let visible = live.scan_all(&format!("{marker}*"), 500).await;
    assert!(
        visible.is_empty(),
        "db 15 must not see keys from db 0, found {visible:?}"
    );

    let _: () = other_conn_mut.del(&marker).await.unwrap();
}

#[tokio::test]
async fn authenticates_when_a_password_is_configured() {
    let Some(url) = configured_url() else {
        eprintln!("skipped: set {ENV} to run the live store tests");
        return;
    };
    if std::env::var("REDISUI_TEST_PASSWORD").is_err() {
        eprintln!("skipped: set REDISUI_TEST_PASSWORD to test authentication");
        return;
    }
    let password = std::env::var("REDISUI_TEST_PASSWORD").unwrap();

    // Set a password via the unauthenticated admin connection.
    let Some(live) = Live::new("auth").await else {
        return;
    };
    let _: () = redis::cmd("CONFIG")
        .arg("SET")
        .arg("requirepass")
        .arg(&password)
        .query_async(&mut live.conn())
        .await
        .expect("set requirepass");

    // A session built with the password can still work.
    let with_secret = Session::new(&args_with_password(&url, &password)).unwrap();
    let conn = with_secret.connect().await.expect("connect with password");
    with_secret
        .ping(conn.clone())
        .await
        .expect("ping with password");

    // A session without one is rejected.
    let without = Session::new(&args_for(&url)).unwrap();
    let error = without
        .connect()
        .await
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(
        error.to_lowercase().contains("auth") || error.to_lowercase().contains("password"),
        "expected an auth failure, got: {error:?}"
    );

    // Put the server back the way we found it.
    let mut conn = conn;
    let _: () = redis::cmd("CONFIG")
        .arg("SET")
        .arg("requirepass")
        .arg("")
        .query_async(&mut conn)
        .await
        .expect("clear requirepass");
}

fn args_with_password(url: &str, password: &str) -> redisui::cli::Args {
    use clap::Parser;
    redisui::cli::Args::parse_from(["redisui", "--url", url, "--password", password])
}
