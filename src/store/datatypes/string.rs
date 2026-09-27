use redis::RedisResult;

use crate::state::{STRING_PREVIEW_BYTES, ValueView};
use crate::store::Conn;
use crate::store::types::decode;

/// `STRLEN` gives the true length; `GETRANGE` gives a bounded preview so a huge
/// values cannot flood the pane.
pub(super) async fn fetch<C: Conn>(conn: &mut C, key: &str) -> RedisResult<ValueView> {
    let bytes: usize = redis::cmd("STRLEN").arg(key).query_async(conn).await?;
    let raw: Vec<u8> = redis::cmd("GETRANGE")
        .arg(key)
        .arg(0)
        .arg(STRING_PREVIEW_BYTES.saturating_sub(1))
        .query_async(conn)
        .await?;
    let (text, binary) = decode(&raw);
    Ok(ValueView::Scalar {
        text,
        bytes,
        binary,
    })
}
