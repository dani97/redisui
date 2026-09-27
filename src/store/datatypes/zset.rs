use redis::{RedisResult, Value};

use crate::state::{VALUE_ROW_LIMIT, ValueView};
use crate::store::Conn;
use crate::store::datatypes::headers;
use crate::store::types::value_to_string;

pub(super) async fn fetch<C: Conn>(conn: &mut C, key: &str) -> RedisResult<ValueView> {
    let total: usize = redis::cmd("ZCARD").arg(key).query_async(conn).await?;
    let flat: Vec<Value> = redis::cmd("ZRANGE")
        .arg(key)
        .arg(0)
        .arg(VALUE_ROW_LIMIT.saturating_sub(1))
        .arg("WITHSCORES")
        .query_async(conn)
        .await?;

    // WITHSCORES replies member, score, member, score, ... so the score trails
    // its member. An unpaired trailing member is dropped.
    let mut rows = Vec::new();
    let mut pending_member: Option<String> = None;
    for value in flat {
        match pending_member.take() {
            None => pending_member = Some(value_to_string(&value)),
            Some(member) => rows.push(vec![value_to_string(&value), member]),
        }
    }

    Ok(ValueView::Rows {
        headers: headers(&["score", "member"]),
        rows,
        total,
    })
}
