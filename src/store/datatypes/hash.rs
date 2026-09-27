use redis::{RedisResult, Value};

use crate::state::{VALUE_ROW_LIMIT, ValueView};
use crate::store::Conn;
use crate::store::datatypes::headers;
use crate::store::types::{decode, scan_pairs};

pub(super) async fn fetch<C: Conn>(conn: &mut C, key: &str) -> RedisResult<ValueView> {
    let total: usize = redis::cmd("HLEN").arg(key).query_async(conn).await?;
    let reply: Value = redis::cmd("HSCAN")
        .arg(key)
        .arg(0)
        .arg("COUNT")
        .arg(VALUE_ROW_LIMIT)
        .query_async(conn)
        .await?;

    let mut rows = Vec::new();
    for (field, value) in scan_pairs(&reply) {
        if rows.len() >= VALUE_ROW_LIMIT {
            break;
        }
        let (field, _) = decode(&field);
        let (value, _) = decode(&value);
        rows.push(vec![field, value]);
    }

    Ok(ValueView::Rows {
        headers: headers(&["field", "value"]),
        rows,
        total,
    })
}
