//! Test scaffolding for the store: a fake async connection that replays canned
//! replies and records exactly what was sent, so tests pin the wire format as
//! well as the parsing.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use redis::aio::ConnectionLike;
use redis::{RedisError, RedisFuture, RedisResult, Value};

use clap::Parser as _;

use crate::cli::Args;
use crate::store::Session;

/// A fake connection. Replies are consumed in order, from one queue shared by
/// the single-command and pipeline paths.
#[derive(Clone, Default)]
pub(crate) struct FakeConn {
    inner: Arc<Mutex<FakeInner>>,
}

#[derive(Default)]
struct FakeInner {
    sent: Vec<String>,
    responses: VecDeque<RedisResult<Value>>,
}

impl FakeConn {
    pub(crate) fn new(replies: impl IntoIterator<Item = Value>) -> Self {
        Self::with_errors(replies.into_iter().map(Ok).collect())
    }

    /// For the cases where a command must fail.
    pub(crate) fn with_errors(responses: Vec<RedisResult<Value>>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(FakeInner {
                sent: Vec::new(),
                responses: responses.into(),
            })),
        }
    }

    /// Every command seen so far, in order, e.g. `STRLEN greeting`.
    pub(crate) fn sent(&self) -> Vec<String> {
        self.inner.lock().unwrap().sent.clone()
    }

    /// Records one command. A pipeline is recorded once, not once per reply.
    fn record(&self, packed: &[u8], label: &str) {
        let command = decode_resp_command(packed);
        self.inner.lock().unwrap().sent.push(if label.is_empty() {
            command
        } else {
            format!("{label}{command}")
        });
    }

    fn pop_reply(&self) -> RedisResult<Value> {
        self.inner
            .lock()
            .unwrap()
            .responses
            .pop_front()
            .unwrap_or_else(|| {
                Err(RedisError::from((
                    redis::ErrorKind::Extension,
                    "fake ran out of canned replies",
                )))
            })
    }
}

impl ConnectionLike for FakeConn {
    fn req_packed_command<'a>(&'a mut self, cmd: &'a redis::Cmd) -> RedisFuture<'a, Value> {
        let packed = cmd.get_packed_command().to_vec();
        Box::pin(async move {
            self.record(&packed, "");
            self.pop_reply()
        })
    }

    /// Serves a batch of replies drawn from the same queue. `offset` is ignored:
    /// every test issues at most one batch per pipeline.
    fn req_packed_commands<'a>(
        &'a mut self,
        pipeline: &'a redis::Pipeline,
        _offset: usize,
        count: usize,
    ) -> RedisFuture<'a, Vec<Value>> {
        let packed = pipeline.get_packed_pipeline();
        let label = format!("PIPELINE({}): ", pipeline.len());
        Box::pin(async move {
            self.record(&packed, &label);
            let mut replies = Vec::with_capacity(count);
            for _ in 0..count {
                replies.push(self.pop_reply()?);
            }
            Ok(replies)
        })
    }

    fn get_db(&self) -> i64 {
        0
    }
}

/// Parses a RESP array of bulk strings into a readable command, so tests can
/// assert on `STRLEN greeting` rather than on protocol bytes. Handles several
/// concatenated arrays, which is what a pipeline looks like on the wire.
pub(crate) fn decode_resp_command(packed: &[u8]) -> String {
    let mut args: Vec<String> = Vec::new();
    let mut pos = 0usize;

    while pos < packed.len() {
        if packed[pos] == b'*' {
            pos = skip_line(packed, pos);
            continue;
        }
        if packed[pos] != b'$' {
            break;
        }
        let len = read_len(packed, &mut pos);
        if pos + len > packed.len() {
            break;
        }
        args.push(String::from_utf8_lossy(&packed[pos..pos + len]).to_string());
        pos += len;
        if packed.len() >= pos + 2 && &packed[pos..pos + 2] == b"\r\n" {
            pos += 2;
        }
    }

    args.join(" ")
}

fn skip_line(packed: &[u8], mut pos: usize) -> usize {
    while pos < packed.len() && packed[pos] != b'\r' {
        pos += 1;
    }
    pos + 2
}

fn read_len(packed: &[u8], pos: &mut usize) -> usize {
    *pos += 1;
    let mut len = 0usize;
    while *pos < packed.len() && packed[*pos] != b'\r' {
        len = len * 10 + (packed[*pos] - b'0') as usize;
        *pos += 1;
    }
    *pos += 2;
    len
}

pub(crate) fn bulk(text: &str) -> Value {
    Value::BulkString(text.as_bytes().to_vec())
}

pub(crate) fn raw(bytes: Vec<u8>) -> Value {
    Value::BulkString(bytes)
}

pub(crate) fn int(number: i64) -> Value {
    Value::Int(number)
}

pub(crate) fn array(items: Vec<Value>) -> Value {
    Value::Array(items)
}

/// A `SCAN`/`HSCAN`/`SSCAN` reply: cursor plus a nested array of items.
pub(crate) fn scan_reply(cursor: &str, items: Vec<Value>) -> Value {
    array(vec![bulk(cursor), array(items)])
}

pub(crate) fn failure(message: &'static str) -> RedisResult<Value> {
    Err(RedisError::from((redis::ErrorKind::Extension, message)))
}

pub(crate) fn nil() -> Value {
    Value::Nil
}

pub(crate) fn session() -> Session {
    session_for("redis://127.0.0.1:6379")
}

pub(crate) fn session_for(url: &str) -> Session {
    Session::new(&Args::parse_from(["redisui", "--url", url])).expect("valid url")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_a_single_command() {
        let packed = b"*2\r\n$6\r\nSTRLEN\r\n$8\r\ngreeting\r\n";
        assert_eq!(decode_resp_command(packed), "STRLEN greeting");
    }

    #[test]
    fn decodes_a_multi_bulk_command() {
        let packed = b"*4\r\n$8\r\nGETRANGE\r\n$3\r\nkey\r\n$1\r\n0\r\n$4\r\n8191";
        assert_eq!(decode_resp_command(packed), "GETRANGE key 0 8191");
    }

    #[test]
    fn decodes_concatenated_arrays_as_written() {
        // Two pipelined TYPE commands, as `redis::pipe` packs them.
        let packed = b"*1\r\n$4\r\nTYPE\r\n$1\r\na*1\r\n$4\r\nTYPE\r\n$1\r\nb";
        assert_eq!(decode_resp_command(packed), "TYPE a TYPE b");
    }
}
