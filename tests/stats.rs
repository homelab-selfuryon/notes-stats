//! `/v1/stats` against a stand-in for notes-api that only answers a caller presenting the current token.

use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex, atomic::Ordering},
};

use anyhow::Result;
use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode, header},
    routing::get,
};
use http_body_util::BodyExt;
use notes_stats::{AppState, config::Config, notes_api::NotesApi, routes};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tower::ServiceExt;

/// The token notes-api currently accepts, and the tokens it has seen.
#[derive(Clone)]
struct Upstream {
    accepts: Arc<Mutex<String>>,
    seen: Arc<Mutex<Vec<String>>>,
    count: i64,
}

async fn count(State(up): State<Upstream>, headers: HeaderMap) -> Result<Json<Value>, StatusCode> {
    let presented = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default()
        .to_owned();
    if let Ok(mut seen) = up.seen.lock() {
        seen.push(presented.clone());
    }
    let accepted = up.accepts.lock().map(|a| a.clone()).unwrap_or_default();
    if presented == accepted {
        Ok(Json(json!({ "count": up.count })))
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

async fn serve(up: Upstream) -> Result<String> {
    let app = Router::new()
        .route("/v1/notes/count", get(count))
        .with_state(up);
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}", listener.local_addr()?);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok(url)
}

fn app(url: &str, token_file: &Path) -> Result<(Router, AppState)> {
    let map: HashMap<String, String> = [
        ("NOTES_API_URL", url.to_owned()),
        ("NOTES_API_TOKEN_FILE", token_file.display().to_string()),
        ("UPSTREAM_TIMEOUT_SECS", "2".to_owned()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    let config = Config::from_lookup(|k| map.get(k).cloned())?;
    let notes_api = NotesApi::new(
        &config.notes_api_url,
        config.notes_api_token_file.clone(),
        config.upstream_timeout,
    )?;
    let state = AppState::new(config, notes_api);
    Ok((routes::router(state.clone()), state))
}

async fn get_json(app: &Router, uri: &str) -> Result<(StatusCode, Value)> {
    let res = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty())?)
        .await?;
    let status = res.status();
    let bytes = res.into_body().collect().await?.to_bytes();
    Ok((
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    ))
}

#[tokio::test]
async fn calls_notes_api_with_the_projected_token() -> Result<()> {
    let up = Upstream {
        accepts: Arc::new(Mutex::new("token-1".into())),
        seen: Arc::default(),
        count: 42,
    };
    let url = serve(up.clone()).await?;
    let dir = tempfile::tempdir()?;
    let token_file = dir.path().join("token");
    std::fs::write(&token_file, "token-1\n")?;
    let (app, _) = app(&url, &token_file)?;

    let (status, body) = get_json(&app, "/v1/stats").await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["notes"]["count"], 42);
    assert_eq!(body["source"], "notes-api");
    Ok(())
}

#[tokio::test]
async fn the_token_is_read_again_on_every_call() -> Result<()> {
    let up = Upstream {
        accepts: Arc::new(Mutex::new("token-1".into())),
        seen: Arc::default(),
        count: 1,
    };
    let url = serve(up.clone()).await?;
    let dir = tempfile::tempdir()?;
    let token_file = dir.path().join("token");
    std::fs::write(&token_file, "token-1")?;
    let (app, _) = app(&url, &token_file)?;

    assert_eq!(get_json(&app, "/v1/stats").await?.0, StatusCode::OK);

    // The kubelet rotates the token; notes-api accepts the new one only.
    std::fs::write(&token_file, "token-2")?;
    assert_eq!(
        get_json(&app, "/v1/stats").await?.0,
        StatusCode::BAD_GATEWAY,
        "stale token refused"
    );
    if let Ok(mut a) = up.accepts.lock() {
        *a = "token-2".into();
    }
    assert_eq!(
        get_json(&app, "/v1/stats").await?.0,
        StatusCode::OK,
        "new token picked up"
    );

    let seen = up.seen.lock().map(|s| s.clone()).unwrap_or_default();
    assert_eq!(seen, ["token-1", "token-2", "token-2"]);
    Ok(())
}

#[tokio::test]
async fn a_missing_token_is_not_sent_as_an_empty_one() -> Result<()> {
    let up = Upstream {
        accepts: Arc::new(Mutex::new(String::new())),
        seen: Arc::default(),
        count: 1,
    };
    let url = serve(up.clone()).await?;
    let dir = tempfile::tempdir()?;
    let (app, _) = app(&url, &dir.path().join("no-such-token"))?;

    let (status, _) = get_json(&app, "/v1/stats").await?;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        up.seen.lock().map(|s| s.is_empty()).unwrap_or(false),
        "nothing was sent"
    );
    Ok(())
}

#[tokio::test]
async fn an_unreachable_notes_api_is_a_bad_gateway() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let token_file = dir.path().join("token");
    std::fs::write(&token_file, "t")?;
    // Nothing listens on port 9 of the loopback.
    let (app, _) = app("http://127.0.0.1:9", &token_file)?;
    let (status, body) = get_json(&app, "/v1/stats").await?;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert!(
        !body.to_string().contains("127.0.0.1"),
        "no internals in the answer"
    );
    Ok(())
}

#[tokio::test]
async fn health_and_readiness() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let (app, state) = app("http://127.0.0.1:9", &dir.path().join("t"))?;
    assert_eq!(get_json(&app, "/healthz").await?.0, StatusCode::OK);
    assert_eq!(
        get_json(&app, "/readyz").await?.0,
        StatusCode::OK,
        "ready without notes-api"
    );
    state.ready.store(false, Ordering::Relaxed);
    assert_eq!(
        get_json(&app, "/readyz").await?.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(get_json(&app, "/healthz").await?.0, StatusCode::OK);
    Ok(())
}
