use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::Deserialize;
use tiny_http::{Method, Response, Server};

use crate::tracking::domain::events::{BrowserKind, TabChange};
use crate::tracking::domain::source_key::strip_query_and_fragment;
use crate::tracking::ports::SessionTracker;

pub const PORT: u16 = 47632;

#[derive(Deserialize)]
struct TabEventDto {
    browser: String,
    url: String,
    title: String,
}

pub fn spawn(tracker: Arc<Mutex<Box<dyn SessionTracker>>>) -> Result<thread::JoinHandle<()>> {
    let server = Server::http(("127.0.0.1", PORT))
        .map_err(|e| anyhow!("binding tab-ingest server to 127.0.0.1:{PORT}: {e}"))?;

    thread::Builder::new()
        .name("makora-tab-ingest".into())
        .spawn(move || run(server, tracker))
        .map_err(|e| anyhow!("spawning tab-ingest server thread: {e}"))
}

fn run(server: Server, tracker: Arc<Mutex<Box<dyn SessionTracker>>>) {
    for mut request in server.incoming_requests() {
        let status = handle_request(&mut request, &tracker);
        let _ = request.respond(Response::empty(status));
    }
}

fn handle_request(request: &mut tiny_http::Request, tracker: &Arc<Mutex<Box<dyn SessionTracker>>>) -> u16 {
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

    let change = TabChange {
        browser: browser_from_wire(&dto.browser),
        url: strip_query_and_fragment(&dto.url),
        title: dto.title,
        at: Utc::now(),
    };

    if let Err(err) = tracker.lock().unwrap().handle_tab(change) {
        tracing::error!(error = format!("{err:#}"), "failed to record tab event");
        return 500;
    }
    204
}

fn browser_from_wire(value: &str) -> BrowserKind {
    match value {
        "chrome" => BrowserKind::Chrome,
        "edge" => BrowserKind::Edge,
        "firefox" => BrowserKind::Firefox,
        "brave" => BrowserKind::Brave,
        _ => BrowserKind::Other,
    }
}

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
