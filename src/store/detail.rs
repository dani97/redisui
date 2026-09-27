use redis::RedisResult;

use crate::state::{DataType, KeyDetail, Ttl};
use crate::store::{Conn, RepoFuture};

pub fn fetch<C: Conn>(conn: C, key: &str) -> RepoFuture<RedisResult<KeyDetail>> {
    let key = key.to_string();
    let mut conn = conn;
    Box::pin(async move {
        let raw_type: String = redis::cmd("TYPE").arg(&key).query_async(&mut conn).await?;
        let Some(data_type) = DataType::from_redis_type(&raw_type) else {
            let reason = if raw_type == "none" {
                "key no longer exists"
            } else {
                "unsupported key type"
            };
            return Err(redis::RedisError::from((
                redis::ErrorKind::UnexpectedReturnType,
                reason,
            )));
        };

        let ttl_seconds: i64 = redis::cmd("TTL").arg(&key).query_async(&mut conn).await?;
        let ttl = match ttl_seconds {
            -2 => Ttl::Unknown,
            -1 => Ttl::Persistent,
            secs => Ttl::Expiring(secs),
        };

        let encoding: String = match redis::cmd("OBJECT")
            .arg("ENCODING")
            .arg(&key)
            .query_async(&mut conn)
            .await
        {
            Ok(value) => value,
            Err(_) => "n/a".to_string(),
        };

        let memory_bytes: Option<u64> = redis::cmd("MEMORY")
            .arg("USAGE")
            .arg(&key)
            .query_async(&mut conn)
            .await
            .ok();

        Ok(KeyDetail {
            data_type,
            ttl,
            encoding,
            memory_bytes,
        })
    })
}
