use redis::{RedisResult, Value};

use crate::state::DataType;
use crate::store::Conn;
use crate::store::types;

pub fn fetch_scan_page<C: Conn>(
    conn: C,
    cursor: u64,
    filter: &str,
    count: usize,
) -> crate::store::RepoFuture<RedisResult<crate::store::ScanPage>> {
    let filter = filter.to_string();
    let mut conn = conn;
    Box::pin(async move {
        let total_keys: u64 = redis::cmd("DBSIZE").query_async(&mut conn).await?;

        let mut command = redis::cmd("SCAN");
        command.arg(cursor).arg("COUNT").arg(count);
        if !filter.is_empty() {
            command.arg("MATCH").arg(&filter);
        }
        let reply: Value = command.query_async(&mut conn).await?;
        let (next, names) = types::cursor_reply(&reply);

        if names.is_empty() {
            return Ok(crate::store::ScanPage {
                cursor: next,
                total_keys,
                done: next == 0,
                keys: Vec::new(),
            });
        }

        let mut pipeline = redis::pipe();
        for name in &names {
            pipeline.cmd("TYPE").arg(name);
        }
        let kind: Vec<Value> = pipeline.query_async(&mut conn).await?;

        let mut keys = Vec::with_capacity(names.len());
        for (name, value) in names.into_iter().zip(kind.iter()) {
            let raw = types::value_to_string(value);
            if let Some(data_type) = DataType::from_redis_type(&raw) {
                let (decoded, _) = types::decode(&name);
                keys.push((data_type, decoded));
            }
        }

        Ok(crate::store::ScanPage {
            cursor: next,
            total_keys,
            done: next == 0,
            keys,
        })
    })
}
