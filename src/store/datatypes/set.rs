use redis::{RedisResult, Value};

use crate::state::{VALUE_ROW_LIMIT, ValueView};
use crate::store::Conn;
use crate::store::datatypes::headers;
use crate::store::types::{decode, scan_members};

pub(super) async fn fetch<C: Conn>(conn: &mut C, key: &str) -> RedisResult<ValueView> {
    let total: usize = redis::cmd("SCARD").arg(key).query_async(conn).await?;
    let reply: Value = redis::cmd("SSCAN")
        .arg(key)
        .arg(0)
        .arg("COUNT")
        .arg(VALUE_ROW_LIMIT)
        .query_async(conn)
        .await?;

    let mut rows = Vec::new();
    for member in scan_members(&reply) {
        if rows.len() >= VALUE_ROW_LIMIT {
            break;
        }
        let (member, _) = decode(&member);
        rows.push(vec![member]);
    }

    Ok(ValueView::Rows {
        headers: headers(&["member"]),
        rows,
        total,
    })
}
