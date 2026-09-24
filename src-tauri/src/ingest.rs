use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::Deserialize;
use tiny_http::{Method, Response, Server};

use crate::core::category::{filter_query_params, normalized_domain};
use crate::core::{BrowserKind, SessionStore, SessionTracker, TabEvent, UrlQueryStore};

pub const PORT: u16 = 47632;

#[derive(Deserialize)]
struct TabEventDto {
    browser: String,
    url: String,
    title: String,
}

pub fn spawn<S: SessionStore + Send + 'static>(
    tracker: Arc<Mutex<SessionTracker<S>>>,
    url_params: Arc<UrlQueryStore>,
) -> Result<thread::JoinHandle<()>> {
    let server = Server::http(("127.0.0.1", PORT))
        .map_err(|e| anyhow!("binding tab-ingest server to 127.0.0.1:{PORT}: {e}"))?;

    thread::Builder::new()
        .name("makora-tab-ingest".into())
        .spawn(move || run(server, tracker, url_params))
        .map_err(|e| anyhow!("spawning tab-ingest server thread: {e}"))
}

fn run<S: SessionStore>(
    server: Server,
    tracker: Arc<Mutex<SessionTracker<S>>>,
    url_params: Arc<UrlQueryStore>,
) {
    for mut request in server.incoming_requests() {
        let status = handle_request(&mut request, &tracker, &url_params);
        let _ = request.respond(Response::empty(status));
    }
}

fn handle_request<S: SessionStore>(
    request: &mut tiny_http::Request,
    tracker: &Arc<Mutex<SessionTracker<S>>>,
    url_params: &UrlQueryStore,
) -> u16 {
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
        url: normalize_url(&dto.url, url_params),
        title: dto.title,
        timestamp: Utc::now(),
    };

    if let Err(err) = tracker.lock().unwrap().handle_tab_event(event) {
        tracing::error!(error = format!("{err:#}"), "failed to record tab event");
        return 500;
    }
    204
}

fn normalize_url(url: &str, url_params: &UrlQueryStore) -> String {
    let allowed = normalized_domain(url)
        .and_then(|domain| url_params.params_for(&domain).ok())
        .unwrap_or_default();
    filter_query_params(url, &allowed)
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
