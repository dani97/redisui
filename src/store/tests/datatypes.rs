use crate::state::{DataType, STRING_PREVIEW_BYTES, VALUE_ROW_LIMIT, ValueView};
use crate::store::{RedisRepository, fetch_value_for};

use super::common::{FakeConn, array, bulk, failure, int, raw, scan_reply, session};

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
async fn fetches_string_value_with_length() {
    let conn = FakeConn::new(vec![int(5), bulk("hello")]);

    let view = fetch_value_for(conn.clone(), "greeting", DataType::String)
        .await
        .unwrap();

    match view {
        ValueView::Scalar {
            text,
            bytes,
            binary,
        } => {
            assert_eq!(text, "hello");
            assert_eq!(bytes, 5);
            assert!(!binary);
        }
        other => panic!("expected scalar, got {other:?}"),
    }
    assert_eq!(
        conn.sent(),
        vec![
            "STRLEN greeting".to_string(),
            format!("GETRANGE greeting 0 {}", STRING_PREVIEW_BYTES - 1)
        ]
    );
}

#[tokio::test]
async fn flags_binary_strings_and_falls_back_to_hex() {
    let conn = FakeConn::new(vec![int(2), raw(vec![0xff, 0x00])]);

    let view = fetch_value_for(conn, "blob", DataType::String)
        .await
        .unwrap();

    match view {
        ValueView::Scalar { text, binary, .. } => {
            assert!(binary);
            assert_eq!(text, "ff00");
        }
        other => panic!("expected scalar, got {other:?}"),
    }
}

#[tokio::test]
async fn fetches_list_rows_with_indices() {
    let conn = FakeConn::new(vec![int(2), array(vec![bulk("first"), bulk("second")])]);

    let (headers, rows, total) = rows_of(
        fetch_value_for(conn.clone(), "queue", DataType::List)
            .await
            .unwrap(),
    );

    assert_eq!(headers, vec!["#", "value"]);
    assert_eq!(rows, vec![vec!["0", "first"], vec!["1", "second"]]);
    assert_eq!(total, 2);
    assert_eq!(
        conn.sent(),
        vec![
            "LLEN queue".to_string(),
            format!("LRANGE queue 0 {}", VALUE_ROW_LIMIT - 1)
        ]
    );
}

#[tokio::test]
async fn fetches_hash_field_value_pairs() {
    let conn = FakeConn::new(vec![
        int(2),
        scan_reply(
            "0",
            vec![bulk("name"), bulk("alice"), bulk("email"), bulk("a@b.c")],
        ),
    ]);

    let (headers, rows, total) = rows_of(
        fetch_value_for(conn.clone(), "user:1", DataType::Hash)
            .await
            .unwrap(),
    );

    assert_eq!(headers, vec!["field", "value"]);
    assert_eq!(rows, vec![vec!["name", "alice"], vec!["email", "a@b.c"]]);
    assert_eq!(total, 2);
    assert_eq!(
        conn.sent(),
        vec![
            "HLEN user:1".to_string(),
            format!("HSCAN user:1 0 COUNT {VALUE_ROW_LIMIT}")
        ]
    );
}

#[tokio::test]
async fn fetches_set_members() {
    let conn = FakeConn::new(vec![
        int(2),
        scan_reply("0", vec![bulk("red"), bulk("blue")]),
    ]);

    let (headers, rows, total) = rows_of(
        fetch_value_for(conn.clone(), "tags", DataType::Set)
            .await
            .unwrap(),
    );

    assert_eq!(headers, vec!["member"]);
    assert_eq!(rows, vec![vec!["red"], vec!["blue"]]);
    assert_eq!(total, 2);
    assert_eq!(
        conn.sent(),
        vec![
            "SCARD tags".to_string(),
            format!("SSCAN tags 0 COUNT {VALUE_ROW_LIMIT}")
        ]
    );
}

#[tokio::test]
async fn pairs_sorted_set_scores_with_members() {
    // ZRANGE ... WITHSCORES replies member, score, member, score, ...
    let conn = FakeConn::new(vec![
        int(2),
        array(vec![bulk("alice"), bulk("1.5"), bulk("bob"), bulk("2")]),
    ]);

    let (headers, rows, total) = rows_of(
        fetch_value_for(conn.clone(), "board", DataType::SortedSet)
            .await
            .unwrap(),
    );

    assert_eq!(headers, vec!["score", "member"]);
    assert_eq!(rows, vec![vec!["1.5", "alice"], vec!["2", "bob"]]);
    assert_eq!(total, 2);
    assert_eq!(
        conn.sent(),
        vec![
            "ZCARD board".to_string(),
            format!("ZRANGE board 0 {} WITHSCORES", VALUE_ROW_LIMIT - 1)
        ]
    );
}

#[tokio::test]
async fn drops_a_trailing_sorted_set_score_with_no_member() {
    let conn = FakeConn::new(vec![
        int(1),
        array(vec![bulk("alice"), bulk("1.5"), bulk("orphan")]),
    ]);

    let (_, rows, _) = rows_of(
        fetch_value_for(conn, "odd", DataType::SortedSet)
            .await
            .unwrap(),
    );

    assert_eq!(rows, vec![vec!["1.5", "alice"]]);
}

#[tokio::test]
async fn decodes_binary_members_inside_a_collection() {
    let conn = FakeConn::new(vec![int(1), scan_reply("0", vec![raw(vec![0xff, 0x00])])]);

    let (_, rows, _) = rows_of(fetch_value_for(conn, "weird", DataType::Set).await.unwrap());

    assert_eq!(rows, vec![vec!["ff00"]]);
}

#[tokio::test]
async fn a_failed_query_surfaces_the_error() {
    let conn = FakeConn::with_errors(vec![failure("WRONGTYPE")]);

    let error = fetch_value_for(conn, "mystery", DataType::Hash)
        .await
        .expect_err("the command failed");

    assert!(error.to_string().contains("WRONGTYPE"), "{error}");
}

#[tokio::test]
async fn a_missing_key_yields_an_empty_view() {
    let conn = FakeConn::new(vec![int(0), array(vec![])]);

    let (headers, rows, total) = rows_of(
        session()
            .fetch_value(conn, "gone", DataType::List)
            .await
            .unwrap(),
    );

    assert_eq!(headers, vec!["#", "value"]);
    assert!(rows.is_empty());
    assert_eq!(total, 0);
}
