use crate::state::DataType;
use crate::store::{RedisRepository, scan::fetch_scan_page};

use super::common::{FakeConn, bulk, failure, int, raw, scan_reply, session};

#[tokio::test]
async fn scan_buckets_keys_by_type_and_terminates() {
    let conn = FakeConn::new(vec![
        int(3), // DBSIZE
        scan_reply("0", vec![bulk("a"), bulk("b"), bulk("c")]),
        // pipelined TYPE for a, b, c
        bulk("string"),
        bulk("hash"),
        bulk("none"),
    ]);

    let page = session().scan(conn.clone(), 0, "", 200).await.unwrap();

    assert_eq!(page.total_keys, 3);
    assert!(page.done, "a zero cursor means the pass is complete");
    // "none" has no DataType, so it is dropped rather than shown.
    assert_eq!(
        page.keys,
        vec![
            (DataType::String, "a".to_string()),
            (DataType::Hash, "b".to_string())
        ]
    );
    assert_eq!(
        conn.sent(),
        vec![
            "DBSIZE".to_string(),
            "SCAN 0 COUNT 200".to_string(),
            "PIPELINE(3): TYPE a TYPE b TYPE c".to_string(),
        ]
    );
}

#[tokio::test]
async fn an_empty_page_skips_the_type_pipeline() {
    let conn = FakeConn::new(vec![int(9), scan_reply("42", vec![])]);

    let page = session().scan(conn.clone(), 0, "", 200).await.unwrap();

    assert_eq!(page.cursor, 42);
    assert!(!page.done, "a non-zero cursor means there is more to fetch");
    assert!(page.keys.is_empty());
    assert_eq!(
        conn.sent(),
        vec!["DBSIZE".to_string(), "SCAN 0 COUNT 200".to_string()],
        "no keys means no TYPE calls"
    );
}

#[tokio::test]
async fn a_filter_is_passed_as_match_and_uses_a_larger_count() {
    let conn = FakeConn::new(vec![int(9), scan_reply("0", vec![])]);

    session()
        .scan(conn.clone(), 0, "user:*", 1000)
        .await
        .unwrap();

    assert_eq!(
        conn.sent(),
        vec![
            "DBSIZE".to_string(),
            "SCAN 0 COUNT 1000 MATCH user:*".to_string(),
        ]
    );
}

#[tokio::test]
async fn the_resume_cursor_is_passed_back_to_redis() {
    let conn = FakeConn::new(vec![
        int(1),
        scan_reply("0", vec![bulk("solo")]),
        bulk("set"),
    ]);

    let page = session().scan(conn.clone(), 42, "", 200).await.unwrap();

    assert!(page.done);
    assert_eq!(page.keys, vec![(DataType::Set, "solo".to_string())]);
    assert_eq!(
        conn.sent(),
        vec![
            "DBSIZE".to_string(),
            "SCAN 42 COUNT 200".to_string(),
            "PIPELINE(1): TYPE solo".to_string(),
        ]
    );
}

#[tokio::test]
async fn decodes_key_names_that_are_not_utf8() {
    let conn = FakeConn::new(vec![
        int(1),
        scan_reply("0", vec![raw(vec![b'k', 0xff])]),
        bulk("string"),
    ]);

    let page = fetch_scan_page(conn, 0, "", 200).await.unwrap();

    assert_eq!(page.keys, vec![(DataType::String, "6bff".to_string())]);
}

#[tokio::test]
async fn a_scan_failure_surfaces_the_error() {
    let conn = FakeConn::with_errors(vec![failure("READONLY")]);

    let error = session()
        .scan(conn, 0, "", 200)
        .await
        .expect_err("the scan failed");

    assert!(error.to_string().contains("READONLY"), "{error}");
}
