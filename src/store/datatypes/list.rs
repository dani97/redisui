use redis::RedisResult;

use crate::state::{VALUE_ROW_LIMIT, ValueView};
use crate::store::Conn;
use crate::store::datatypes::headers;
use crate::store::types::decode;

pub(super) async fn fetch<C: Conn>(conn: &mut C, key: &str) -> RedisResult<ValueView> {
    let total: usize = redis::cmd("LLEN").arg(key).query_async(conn).await?;
    let raw: Vec<Vec<u8>> = redis::cmd("LRANGE")
        .arg(key)
        .arg(0)
        .arg(VALUE_ROW_LIMIT.saturating_sub(1))
        .query_async(conn)
        .await?;
    let rows = raw
        .into_iter()
        .enumerate()
        .map(|(index, bytes)| {
            let (text, _) = decode(&bytes);
            vec![index.to_string(), text]
        })
        .collect();
    Ok(ValueView::Rows {
        headers: headers(&["#", "value"]),
        rows,
        total,
    })
}
