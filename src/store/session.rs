use std::fmt;
use std::time::Duration;

use anyhow::Context;
use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use redis::{Client, RedisResult};

use crate::cli::Args;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct ServerInfo {
    pub version: String,
}

#[derive(Clone)]
pub struct Session {
    client: Client,
    pub url: String,
    /// Safe to render: never contains a password.
    pub display_url: String,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("url", &self.display_url)
            .finish()
    }
}

impl Session {
    pub fn new(args: &Args) -> anyhow::Result<Self> {
        let mut info = redis::IntoConnectionInfo::into_connection_info(args.url.as_str())?;
        let mut settings = info.redis_settings().clone();

        if let Some(db) = args.db {
            settings = settings.set_db(i64::from(db));
        }
        if let Some(username) = &args.username {
            settings = settings.set_username(username);
        }
        if let Some(password) = resolve_password(args)? {
            settings = settings.set_password(password);
        }

        info = info.set_redis_settings(settings);
        Ok(Self {
            client: Client::open(info)?,
            url: args.url.clone(),
            display_url: redact_url(&args.url),
        })
    }

    pub async fn connect(&self) -> RedisResult<ConnectionManager> {
        let config = ConnectionManagerConfig::new()
            .set_connection_timeout(Some(CONNECT_TIMEOUT))
            .set_response_timeout(None)
            // We do our own retrying on a timer so that failures are shown to
            // the user. Letting the manager retry internally would swallow a
            // wrong password behind an endless spinner.
            .set_number_of_retries(0);
        ConnectionManager::new_with_config(self.client.clone(), config).await
    }
}

/// Flags and env vars win over credentials embedded in the URL, because they are
/// the more local, more deliberate signal.
fn resolve_password(args: &Args) -> anyhow::Result<Option<String>> {
    if let Some(password) = &args.password {
        return Ok(Some(password.clone()));
    }

    let Some(path) = &args.password_file else {
        return Ok(None);
    };

    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("reading password file {}", path.display()))?;
    let password = trim_trailing_newline(&raw);
    if password.is_empty() {
        anyhow::bail!("password file {} is empty", path.display());
    }
    Ok(Some(password.to_string()))
}

/// Only a trailing newline is stripped: a password may legitimately begin or
/// end with other whitespace.
fn trim_trailing_newline(raw: &str) -> &str {
    raw.trim_end_matches(['\r', '\n'])
}

/// Masks the password in a connection URL so it can be shown in the status bar
/// and error messages. The username is kept, since it is useful when debugging
/// ACLs and is not itself a secret.
///
/// Any literal `@` inside a password arrives percent-encoded as `%40`, so the
/// last `@` before the host is always the delimiter.
pub fn redact_url(url: &str) -> String {
    let Some(scheme_end) = url.find("://").map(|index| index + 3) else {
        return url.to_string();
    };

    let rest = &url[scheme_end..];
    let path_start = rest.find('/').unwrap_or(rest.len());
    let (authority, path) = rest.split_at(path_start);

    let Some(at) = authority.rfind('@') else {
        return url.to_string();
    };

    let (userinfo, host_and_path) = authority.split_at(at);
    let masked = match userinfo.find(':') {
        Some(colon) => format!("{}:***", &userinfo[..colon]),
        None => "***".to_string(),
    };

    format!("{}{masked}{host_and_path}{path}", &url[..scheme_end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn session(args: &[&str]) -> Session {
        let args = Args::parse_from(std::iter::once("redisui").chain(args.iter().copied()));
        Session::new(&args).expect("session")
    }

    fn settings(session: &Session) -> &redis::RedisConnectionInfo {
        session.client.get_connection_info().redis_settings()
    }

    #[test]
    fn keeps_url_credentials_when_no_flags_are_given() {
        let session = session(&["--url", "redis://:fromurl@127.0.0.1:6379"]);
        assert_eq!(settings(&session).password(), Some("fromurl"));
    }

    #[test]
    fn a_password_flag_overrides_the_url() {
        let session = session(&["--url", "redis://:fromurl@127.0.0.1:6379", "-p", "fromflag"]);
        assert_eq!(settings(&session).password(), Some("fromflag"));
    }

    #[test]
    fn applies_username_and_db() {
        let session = session(&["--url", "redis://127.0.0.1:6379/2", "-a", "app", "-d", "5"]);
        assert_eq!(settings(&session).username(), Some("app"));
        assert_eq!(settings(&session).db(), 5);
    }

    #[test]
    fn leaves_the_database_alone_when_db_is_not_given() {
        let session = session(&["--url", "redis://127.0.0.1:6379/7"]);
        assert_eq!(settings(&session).db(), 7);
    }

    #[test]
    fn reads_a_password_file_and_ignores_one_trailing_newline() {
        let dir = std::env::temp_dir().join("redisui-pw-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pw");
        std::fs::write(&path, " filepass \n").unwrap();

        let session = session(&["--password-file", path.to_str().unwrap()]);

        assert_eq!(
            settings(&session).password(),
            Some(" filepass "),
            "only the newline is stripped; spaces are part of the password"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_password_file_may_have_no_trailing_newline() {
        let dir = std::env::temp_dir().join("redisui-pw-test-nonl");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pw");
        std::fs::write(&path, "nopad").unwrap();

        let session = session(&["--password-file", path.to_str().unwrap()]);
        assert_eq!(settings(&session).password(), Some("nopad"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_an_empty_password_file() {
        let dir = std::env::temp_dir().join("redisui-pw-test-empty");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pw");
        std::fs::write(&path, "\n").unwrap();

        let args = Args::parse_from(["redisui", "--password-file", path.to_str().unwrap()]);
        let error = Session::new(&args).expect_err("an empty file is a mistake");

        assert!(error.to_string().contains("is empty"), "{error}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reports_a_missing_password_file_clearly() {
        let args = Args::parse_from(["redisui", "--password-file", "/nonexistent/redisui/pw"]);
        let error = Session::new(&args).expect_err("the file is missing");
        assert!(
            error.to_string().contains("reading password file"),
            "{error}"
        );
    }

    #[test]
    fn display_url_never_contains_a_password() {
        let session = session(&["--url", "redis://:supersecret@127.0.0.1:6379"]);
        assert!(!session.display_url.contains("supersecret"));
        assert!(!format!("{session:?}").contains("supersecret"));
    }

    #[test]
    fn redacts_a_password_only_url() {
        assert_eq!(
            redact_url("redis://:hunter2@cache:6379"),
            "redis://:***@cache:6379"
        );
    }

    #[test]
    fn redacts_while_keeping_the_username() {
        assert_eq!(
            redact_url("redis://app:hunter2@cache:6379/3"),
            "redis://app:***@cache:6379/3"
        );
    }

    #[test]
    fn redacts_a_username_only_url() {
        assert_eq!(
            redact_url("redis://app@cache:6379"),
            "redis://***@cache:6379"
        );
    }

    #[test]
    fn leaves_a_url_without_credentials_alone() {
        assert_eq!(redact_url("redis://cache:6379"), "redis://cache:6379");
        assert_eq!(
            redact_url("redis+unix:///tmp/redis.sock"),
            "redis+unix:///tmp/redis.sock"
        );
    }

    #[test]
    fn redacts_even_when_the_host_looks_like_userinfo() {
        assert_eq!(redact_url("redis://:pw@host/0"), "redis://:***@host/0");
    }
}
