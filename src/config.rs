//! Non-secret configuration, from environment variables. The platform sets `PORT`, `APP_ENV`,
//! `APP_VERSION` and, for every entry of `network.calls`, `<CALLEE>_URL` (here `NOTES_API_URL`);
//! anything else comes from `env:` in `deploy/deploy.cue`.

use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result};

/// Where the platform mounts the token for notes-api (`network.calls`, audience `notes-api`).
pub const DEFAULT_TOKEN_FILE: &str = "/var/run/secrets/tokens/notes-api/token";
/// Same-namespace name; the platform sets the full URL.
pub const DEFAULT_NOTES_API_URL: &str = "http://notes-api";

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub app_env: String,
    pub app_version: String,
    /// How long to keep serving after SIGTERM with `/readyz` failing.
    pub shutdown_delay: Duration,
    /// Upper bound of a request to this app.
    pub request_timeout: Duration,
    /// Base URL of notes-api, without a trailing slash.
    pub notes_api_url: String,
    /// The projected token for notes-api; read on every call.
    pub notes_api_token_file: PathBuf,
    /// Upper bound of the call to notes-api.
    pub upstream_timeout: Duration,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Reads settings through `get`; tests pass a map.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let port = match get("PORT") {
            Some(v) => v.parse().with_context(|| format!("PORT={v:?}"))?,
            None => 8080,
        };
        let secs = |key: &str, default: u64| -> Result<Duration> {
            match get(key) {
                Some(v) => Ok(Duration::from_secs(
                    v.parse().with_context(|| format!("{key}={v:?}"))?,
                )),
                None => Ok(Duration::from_secs(default)),
            }
        };
        Ok(Self {
            port,
            app_env: get("APP_ENV").unwrap_or_else(|| "local".to_owned()),
            app_version: get("APP_VERSION").unwrap_or_else(|| "dev".to_owned()),
            shutdown_delay: secs("SHUTDOWN_DELAY_SECS", 2)?,
            request_timeout: secs("REQUEST_TIMEOUT_SECS", 30)?,
            notes_api_url: get("NOTES_API_URL")
                .unwrap_or_else(|| DEFAULT_NOTES_API_URL.to_owned())
                .trim_end_matches('/')
                .to_owned(),
            notes_api_token_file: get("NOTES_API_TOKEN_FILE")
                .unwrap_or_else(|| DEFAULT_TOKEN_FILE.to_owned())
                .into(),
            upstream_timeout: secs("UPSTREAM_TIMEOUT_SECS", 3)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn cfg(pairs: &[(&str, &str)]) -> Result<Config> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        Config::from_lookup(|k| map.get(k).cloned())
    }

    #[test]
    fn defaults() -> Result<()> {
        let c = cfg(&[])?;
        assert_eq!(c.port, 8080);
        assert_eq!(c.notes_api_url, "http://notes-api");
        assert_eq!(
            c.notes_api_token_file,
            PathBuf::from("/var/run/secrets/tokens/notes-api/token")
        );
        Ok(())
    }

    #[test]
    fn url_from_the_platform() -> Result<()> {
        let c = cfg(&[(
            "NOTES_API_URL",
            "http://notes-api.notes-prod.svc.cluster.local/",
        )])?;
        assert_eq!(
            c.notes_api_url,
            "http://notes-api.notes-prod.svc.cluster.local"
        );
        Ok(())
    }

    #[test]
    fn bad_port() {
        assert!(cfg(&[("PORT", "http")]).is_err());
    }
}
