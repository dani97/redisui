use redis::RedisResult;

use crate::state::{DataType, ValueView};
use crate::store::{Conn, RepoFuture};

pub(crate) mod hash;
pub(crate) mod list;
pub(crate) mod set;
pub(crate) mod string;
pub(crate) mod zset;

/// The single dispatch point for reading a value. Adding a datatype means adding
/// a variant to `DataType` plus an arm here.
pub fn fetch_value_for<C: Conn>(
    conn: C,
    key: &str,
    data_type: DataType,
) -> RepoFuture<RedisResult<ValueView>> {
    let key = key.to_string();
    let mut conn = conn;
    Box::pin(async move {
        match data_type {
            DataType::String => string::fetch(&mut conn, &key).await,
            DataType::List => list::fetch(&mut conn, &key).await,
            DataType::Hash => hash::fetch(&mut conn, &key).await,
            DataType::Set => set::fetch(&mut conn, &key).await,
            DataType::SortedSet => zset::fetch(&mut conn, &key).await,
        }
    })
}

pub(super) fn headers(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}
