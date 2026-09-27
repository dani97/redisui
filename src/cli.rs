use std::path::PathBuf;

use clap::Parser;

pub const DEFAULT_URL: &str = "redis://127.0.0.1:6379";

#[derive(Debug, Parser)]
#[command(name = "redisui", version, about = "A terminal UI browser for Redis")]
pub struct Args {
    /// Connection URL. May embed credentials as redis://[:password@]host.
    #[arg(
        short,
        long,
        env = "REDIS_URL",
        default_value = DEFAULT_URL,
        value_name = "URL"
    )]
    pub url: String,

    /// Redis database index. Only applied when given, so an index in the URL
    /// is otherwise respected.
    #[arg(short, long, value_name = "INDEX")]
    pub db: Option<u32>,

    /// ACL username. Omit for password-only (legacy) authentication.
    #[arg(short = 'a', long, env = "REDIS_USERNAME", value_name = "USER")]
    pub username: Option<String>,

    /// Password. Prefer --password-file or REDIS_PASSWORD: an argument is
    /// visible to anyone who can list processes.
    #[arg(
        short,
        long,
        env = "REDIS_PASSWORD",
        value_name = "PASSWORD",
        hide_env_values = true,
        conflicts_with = "password_file"
    )]
    pub password: Option<String>,

    /// Read the password from a file. One trailing newline is ignored, so
    /// `echo secret > file` works.
    #[arg(long, value_name = "PATH", conflicts_with = "password")]
    pub password_file: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory as _;

    use super::*;

    fn parse(args: &[&str]) -> Args {
        Args::parse_from(std::iter::once("redisui").chain(args.iter().copied()))
    }

    #[test]
    fn defaults_to_local_redis() {
        let args = parse(&[]);
        assert_eq!(args.url, DEFAULT_URL);
        assert_eq!(args.db, None);
        assert_eq!(args.username, None);
        assert_eq!(args.password, None);
        assert_eq!(args.password_file, None);
    }

    #[test]
    fn reads_url_and_db_from_flags() {
        let args = parse(&["--url", "redis://example:6380", "--db", "3"]);
        assert_eq!(args.url, "redis://example:6380");
        assert_eq!(args.db, Some(3));
    }

    #[test]
    fn reads_credentials_from_flags() {
        let args = parse(&["-a", "app", "-p", "s3cret"]);
        assert_eq!(args.username.as_deref(), Some("app"));
        assert_eq!(args.password.as_deref(), Some("s3cret"));
    }

    #[test]
    fn reads_a_password_file_path() {
        let args = parse(&["--password-file", "/run/secrets/redis"]);
        assert_eq!(
            args.password_file.as_deref(),
            Some(std::path::Path::new("/run/secrets/redis"))
        );
        assert_eq!(args.password, None);
    }

    #[test]
    fn password_and_password_file_are_mutually_exclusive() {
        let error = Args::try_parse_from(["redisui", "-p", "a", "--password-file", "/tmp/p"])
            .expect_err("clap should reject both");
        assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn help_output_never_echoes_the_password() {
        let help = Args::command().render_long_help().to_string();
        assert!(!help.contains("s3cret"));
        assert!(help.contains("--password"));
    }
}
