//! The client for notes-api.

use std::path::PathBuf;

use anyhow::{Context, Result};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

/// Why a call failed; the route maps it to a status.
#[derive(Debug, thiserror::Error)]
pub enum CallError {
    /// The projected token cannot be read: the volume is missing or the pod is not allowed to call.
    #[error("token unavailable: {0}")]
    Token(String),
    /// notes-api cannot be reached, or answered garbage.
    #[error("notes-api unreachable: {0}")]
    Unreachable(String),
    /// notes-api answered with this status (401/403: the token was refused).
    #[error("notes-api answered {0}")]
    Status(u16),
}

pub struct NotesApi {
    client: reqwest::Client,
    base_url: String,
    token_file: PathBuf,
}

#[derive(Deserialize)]
struct Count {
    count: i64,
}

impl NotesApi {
    pub fn new(base_url: &str, token_file: PathBuf, timeout: std::time::Duration) -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .context("building the HTTP client")?,
            base_url: base_url.trim_end_matches('/').to_owned(),
            token_file,
        })
    }

    /// The token, read from disk every time: the kubelet rotates it before it expires.
    async fn token(&self) -> Result<SecretString, CallError> {
        let raw = tokio::fs::read_to_string(&self.token_file)
            .await
            .map_err(|e| CallError::Token(format!("{}: {e}", self.token_file.display())))?;
        Ok(SecretString::from(raw.trim().to_owned()))
    }

    /// Number of notes: `GET /v1/notes/count`.
    pub async fn count(&self) -> Result<i64, CallError> {
        let token = self.token().await?;
        let response = self
            .client
            .get(format!("{}/v1/notes/count", self.base_url))
            .bearer_auth(token.expose_secret())
            .send()
            .await
            .map_err(|e| CallError::Unreachable(e.without_url().to_string()))?;
        if !response.status().is_success() {
            return Err(CallError::Status(response.status().as_u16()));
        }
        let count: Count = response
            .json()
            .await
            .map_err(|e| CallError::Unreachable(format!("unexpected answer: {e}")))?;
        Ok(count.count)
    }
}
