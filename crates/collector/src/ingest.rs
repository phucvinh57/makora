use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::Deserialize;
use tiny_http::{Method, Response, Server};

use makora_core::{BrowserKind, SessionTracker, SqliteStore, TabEvent};

/// Loopback port the browser extension posts active-tab events to. Fixed in
/// v1 — there's no config file yet, and the extension has this port baked
/// in too.
pub const PORT: u16 = 47632;

#[derive(Deserialize)]
struct TabEventDto {
    browser: String,
    url: String,
    title: String,
}

/// Starts the local HTTP ingest server on its own thread and returns once
/// it's bound, mirroring how `FocusWatcher` impls own their event loop.
///
/// Bound to loopback only, and every request is checked against an
/// extension-origin allowlist — there's no shared-secret auth in v1, so
/// that's the only thing stopping an arbitrary local process or web page
/// from posting fake tab events.
pub fn spawn(tracker: Arc<Mutex<SessionTracker<SqliteStore>>>) -> Result<thread::JoinHandle<()>> {
    let server = Server::http(("127.0.0.1", PORT))
        .map_err(|e| anyhow!("binding tab-ingest server to 127.0.0.1:{PORT}: {e}"))?;

    thread::Builder::new()
        .name("makora-tab-ingest".into())
        .spawn(move || run(server, tracker))
        .map_err(|e| anyhow!("spawning tab-ingest server thread: {e}"))
}

fn run(server: Server, tracker: Arc<Mutex<SessionTracker<SqliteStore>>>) {
    for mut request in server.incoming_requests() {
        let status = handle_request(&mut request, &tracker);
        let _ = request.respond(Response::empty(status));
    }
}

fn handle_request(request: &mut tiny_http::Request, tracker: &Arc<Mutex<SessionTracker<SqliteStore>>>) -> u16 {
    if *request.method() != Method::Post || request.url() != "/event" {
        return 404;
    }
    if !has_allowed_extension_origin(request) {
        return 403;
    }

    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        return 400;
    }

    let dto: TabEventDto = match serde_json::from_str(&body) {
        Ok(dto) => dto,
        Err(_) => return 400,
    };

    let event = TabEvent {
        browser: BrowserKind::from_wire(&dto.browser),
        url: normalize_url(&dto.url),
        title: dto.title,
        timestamp: Utc::now(),
    };

    if let Err(err) = tracker.lock().unwrap().handle_tab_event(event) {
        eprintln!("makora: failed to record tab event: {err:#}");
        return 500;
    }
    204
}

/// The extension already strips the query string and fragment before
/// sending, but re-strip defensively here so a compromised or misbehaving
/// extension build can't leak tokens through the query string.
fn normalize_url(url: &str) -> String {
    url.split(['?', '#']).next().unwrap_or(url).to_string()
}

/// Loopback binding keeps other machines out, but any local process can
/// still hit a local port — this Origin check keeps ordinary web pages
/// (and non-extension local processes) from injecting fake tab events.
fn has_allowed_extension_origin(request: &tiny_http::Request) -> bool {
    request
        .headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case("origin"))
        .map(|h| {
            let origin = h.value.as_str();
            origin.starts_with("chrome-extension://") || origin.starts_with("moz-extension://")
        })
        .unwrap_or(false)
}
