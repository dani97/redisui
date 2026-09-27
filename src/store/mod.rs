use std::future::Future;
use std::pin::Pin;

use redis::RedisResult;
use redis::aio::{ConnectionLike, ConnectionManager};

use crate::state::{DataType, KeyDetail, ValueView};

pub mod datatypes;
pub mod detail;
pub mod scan;
pub mod session;
pub mod types;

#[cfg(test)]
mod tests;

pub use datatypes::fetch_value_for;
pub use session::{ServerInfo, Session};

pub type RepoFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// Anything that can stand in for a live connection. Generic over the concrete
/// type so the queries can also be driven by the test fake in `tests::common`.
pub trait Conn: ConnectionLike + Clone + Send + Sync + 'static {}

impl<T> Conn for T where T: ConnectionLike + Clone + Send + Sync + 'static {}

#[derive(Debug)]
pub struct ScanPage {
    pub cursor: u64,
    pub total_keys: u64,
    pub done: bool,
    pub keys: Vec<(DataType, String)>,
}

pub trait RedisRepository: Send + Sync {
    fn connect(&self) -> RepoFuture<RedisResult<(ConnectionManager, ServerInfo)>>;

    fn scan<C: Conn>(
        &self,
        conn: C,
        cursor: u64,
        filter: &str,
        count: usize,
    ) -> RepoFuture<RedisResult<ScanPage>>;

    fn fetch_value<C: Conn>(
        &self,
        conn: C,
        key: &str,
        data_type: DataType,
    ) -> RepoFuture<RedisResult<ValueView>>;

    fn fetch_detail<C: Conn>(&self, conn: C, key: &str) -> RepoFuture<RedisResult<KeyDetail>>;

    fn ping<C: Conn>(&self, conn: C) -> RepoFuture<RedisResult<()>>;
}

impl RedisRepository for Session {
    fn connect(&self) -> RepoFuture<RedisResult<(ConnectionManager, ServerInfo)>> {
        let session = self.clone();
        Box::pin(async move {
            let mut conn = session.connect().await?;
            let info: redis::InfoDict = redis::cmd("INFO")
                .arg("server")
                .query_async(&mut conn)
                .await?;
            let server = ServerInfo {
                version: info
                    .get::<String>("redis_version")
                    .unwrap_or_else(|| "unknown".to_string()),
            };
            Ok((conn, server))
        })
    }

    fn scan<C: Conn>(
        &self,
        conn: C,
        cursor: u64,
        filter: &str,
        count: usize,
    ) -> RepoFuture<RedisResult<ScanPage>> {
        scan::fetch_scan_page(conn, cursor, filter, count)
    }

    fn fetch_value<C: Conn>(
        &self,
        conn: C,
        key: &str,
        data_type: DataType,
    ) -> RepoFuture<RedisResult<ValueView>> {
        fetch_value_for(conn, key, data_type)
    }

    fn fetch_detail<C: Conn>(&self, conn: C, key: &str) -> RepoFuture<RedisResult<KeyDetail>> {
        detail::fetch(conn, key)
    }

    fn ping<C: Conn>(&self, conn: C) -> RepoFuture<RedisResult<()>> {
        let mut conn = conn;
        Box::pin(async move {
            let _: String = redis::cmd("PING").query_async(&mut conn).await?;
            Ok(())
        })
    }
}
