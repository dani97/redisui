use crate::state::{DataType, Ttl};
use crate::store::RedisRepository;

use super::common::{FakeConn, bulk, failure, int, nil, session};

#[tokio::test]
async fn reads_key_detail_including_ttl_and_memory() {
    let conn = FakeConn::new(vec![bulk("hash"), int(120), bulk("listpack"), int(128)]);

    let detail = session()
        .fetch_detail(conn.clone(), "user:1")
        .await
        .unwrap();

    assert_eq!(detail.data_type, DataType::Hash);
    assert_eq!(detail.ttl, Ttl::Expiring(120));
    assert_eq!(detail.encoding, "listpack");
    assert_eq!(detail.memory_bytes, Some(128));
    assert_eq!(
        conn.sent(),
        vec![
            "TYPE user:1".to_string(),
            "TTL user:1".to_string(),
            "OBJECT ENCODING user:1".to_string(),
            "MEMORY USAGE user:1".to_string(),
        ]
    );
}

#[tokio::test]
async fn reports_persistent_keys_and_missing_memory() {
    let conn = FakeConn::new(vec![bulk("string"), int(-1), bulk("embstr"), nil()]);

    let detail = session().fetch_detail(conn, "plain").await.unwrap();

    assert_eq!(detail.ttl, Ttl::Persistent);
    assert_eq!(detail.memory_bytes, None);
}

#[tokio::test]
async fn reports_an_expired_key_as_unknown_ttl() {
    let conn = FakeConn::new(vec![bulk("string"), int(-2), bulk("embstr"), int(64)]);

    let detail = session().fetch_detail(conn, "stale").await.unwrap();

    assert_eq!(detail.ttl, Ttl::Unknown);
    assert_eq!(detail.ttl.label(), "key gone");
}

#[tokio::test]
async fn tolerates_object_encoding_being_unavailable() {
    let conn = FakeConn::with_errors(vec![
        Ok(bulk("string")),
        Ok(int(-1)),
        failure("unknown subcommand"),
    ]);

    let detail = session().fetch_detail(conn, "plain").await.unwrap();

    assert_eq!(detail.encoding, "n/a");
}

#[tokio::test]
async fn reports_a_vanished_key_distinctly() {
    let conn = FakeConn::new(vec![bulk("none")]);

    let error = session()
        .fetch_detail(conn, "gone")
        .await
        .expect_err("the key expired");

    assert!(
        error.to_string().contains("key no longer exists"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn rejects_types_outside_the_supported_set() {
    let conn = FakeConn::new(vec![bulk("ReJSON-RL")]);

    let error = session()
        .fetch_detail(conn, "doc:1")
        .await
        .expect_err("module types are not supported");

    assert!(
        error.to_string().contains("unsupported key type"),
        "unexpected error: {error}"
    );
}
