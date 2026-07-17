use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::Deserialize;
use tiny_http::{Method, Response, Server};

use crate::core::category::{filter_query_params, normalized_domain};
use crate::core::{BrowserKind, QueryWhitelistStore, SessionStore, SessionTracker, TabEvent};

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
pub fn spawn<S: SessionStore + Send + 'static>(
    tracker: Arc<Mutex<SessionTracker<S>>>,
    whitelist: Arc<QueryWhitelistStore>,
) -> Result<thread::JoinHandle<()>> {
    let server = Server::http(("127.0.0.1", PORT))
        .map_err(|e| anyhow!("binding tab-ingest server to 127.0.0.1:{PORT}: {e}"))?;

    thread::Builder::new()
        .name("makora-tab-ingest".into())
        .spawn(move || run(server, tracker, whitelist))
        .map_err(|e| anyhow!("spawning tab-ingest server thread: {e}"))
}

fn run<S: SessionStore>(
    server: Server,
    tracker: Arc<Mutex<SessionTracker<S>>>,
    whitelist: Arc<QueryWhitelistStore>,
) {
    for mut request in server.incoming_requests() {
        let status = handle_request(&mut request, &tracker, &whitelist);
        let _ = request.respond(Response::empty(status));
    }
}

fn handle_request<S: SessionStore>(
    request: &mut tiny_http::Request,
    tracker: &Arc<Mutex<SessionTracker<S>>>,
    whitelist: &QueryWhitelistStore,
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
        url: normalize_url(&dto.url, whitelist),
        title: dto.title,
        timestamp: Utc::now(),
    };

    if let Err(err) = tracker.lock().unwrap().handle_tab_event(event) {
        eprintln!("makora: failed to record tab event: {err:#}");
        return 500;
    }
    204
}

/// The privacy boundary for a tab event's query string: the extension now
/// sends it as-is (see `extension/background.js`), so this is the only
/// place deciding what actually gets stored. The fragment is always
/// stripped; the query string is filtered down to whatever params are
/// whitelisted for the URL's domain (see `QueryWhitelistStore`) — a
/// compromised or misbehaving extension build can't leak anything beyond
/// that, since nothing here trusts it to have filtered anything itself.
fn normalize_url(url: &str, whitelist: &QueryWhitelistStore) -> String {
    let allowed = normalized_domain(url)
        .map(|domain| whitelist.allowed_params(&domain))
        .unwrap_or_default();
    filter_query_params(url, &allowed)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::store::sqlite::open_connection;

    fn temp_whitelist() -> (QueryWhitelistStore, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "makora-ingest-normalize-test-{}.sqlite",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
                ^ (std::process::id() as u128) << 64
        ));
        let conn = open_connection(&path).expect("opening test database");
        (QueryWhitelistStore::new(conn), path)
    }

    #[test]
    fn normalize_url_keeps_only_whitelisted_params_for_the_urls_domain() {
        let (whitelist, path) = temp_whitelist();
        assert_eq!(
            normalize_url("https://www.youtube.com/watch?v=XYZ&list=abc", &whitelist),
            "https://www.youtube.com/watch?v=XYZ"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn normalize_url_strips_query_entirely_for_a_domain_with_no_whitelist_entry() {
        let (whitelist, path) = temp_whitelist();
        assert_eq!(
            normalize_url("https://example.com/page?token=secret", &whitelist),
            "https://example.com/page"
        );
        let _ = std::fs::remove_file(&path);
    }
}
