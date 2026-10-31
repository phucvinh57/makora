use std::io;
use std::sync::{Arc, Mutex};
use std::thread;

use chrono::{DateTime, Utc};
use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{Listener, ListenerOptions, Stream};
use serde::Deserialize;
use tracing::{error, info, warn};
use url::Url;

use super::bridge_wire::{read_frame, socket_name, Frame};
use crate::shared::sync::lock;
use crate::tracking::domain::events::{BrowserKind, TabChange};
use crate::tracking::ports::SessionTracker;

const MAX_TITLE_CHARS: usize = 1024;

#[derive(Debug, PartialEq, Eq)]
enum InvalidTabEvent {
    Malformed(String),
    UnknownBrowser(String),
}

/// Listens for the `makora-bridge` native-messaging host. Without the listener the app keeps
/// running and browsers are tracked as plain apps.
pub fn spawn(tracker: Arc<Mutex<Box<dyn SessionTracker>>>) {
    let listener = match bind() {
        Ok(listener) => listener,
        Err(err) => {
            warn!(error = %err, "browser tab tracking disabled: cannot listen for the browser bridge");
            return;
        }
    };
    let spawned = thread::Builder::new()
        .name("makora-tab-ingest".into())
        .spawn(move || accept(listener, tracker));
    match spawned {
        Ok(_) => info!("listening for browser tab events"),
        Err(err) => warn!(error = %err, "browser tab tracking disabled: cannot spawn the listener thread"),
    }
}

#[cfg(unix)]
fn bind() -> io::Result<Listener> {
    use std::fs::{self, DirBuilder, Permissions};
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    let dir = super::bridge_wire::socket_dir()?;
    DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
    fs::set_permissions(&dir, Permissions::from_mode(0o700))?;
    ListenerOptions::new().name(socket_name()?).try_overwrite(true).create_sync()
}

#[cfg(windows)]
fn bind() -> io::Result<Listener> {
    use interprocess::os::windows::local_socket::ListenerOptionsExt;
    use interprocess::os::windows::security_descriptor::SecurityDescriptor;
    use widestring::U16CString;

    let sddl = U16CString::from_str(format!("D:P(A;;GA;;;{})", super::bridge_wire::user_sid()?))
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
    ListenerOptions::new()
        .name(socket_name()?)
        .security_descriptor(SecurityDescriptor::deserialize(&sddl)?)
        .create_sync()
}

fn accept(listener: Listener, tracker: Arc<Mutex<Box<dyn SessionTracker>>>) {
    for connection in listener.incoming() {
        let stream = match connection {
            Ok(stream) => stream,
            Err(err) => {
                warn!(error = %err, "failed to accept a browser bridge connection");
                continue;
            }
        };
        let tracker = tracker.clone();
        let spawned = thread::Builder::new()
            .name("makora-tab-bridge".into())
            .spawn(move || serve(stream, &tracker));
        if let Err(err) = spawned {
            warn!(error = %err, "failed to spawn a browser bridge thread");
        }
    }
}

fn serve(mut stream: Stream, tracker: &Mutex<Box<dyn SessionTracker>>) {
    loop {
        match read_frame(&mut stream) {
            Ok(Some(Frame::Message(payload))) => handle(&payload, tracker),
            Ok(Some(Frame::Oversized)) => warn!("dropped an oversized browser tab event"),
            Ok(None) => return,
            Err(err) => {
                warn!(error = %err, "browser bridge connection broke");
                return;
            }
        }
    }
}

fn handle(payload: &[u8], tracker: &Mutex<Box<dyn SessionTracker>>) {
    let change = match parse_tab_event(payload, Utc::now()) {
        Ok(change) => change,
        Err(err) => {
            warn!(?err, "dropped an invalid browser tab event");
            return;
        }
    };
    if let Err(err) = lock(tracker).handle_tab(change) {
        error!(error = format!("{err:#}"), "failed to record tab event");
    }
}

#[derive(Deserialize)]
struct TabEventDto {
    browser: String,
    url: Option<String>,
    title: String,
}

/// `at` is when the app received the event: the extension's clock and queueing are not trusted.
fn parse_tab_event(payload: &[u8], at: DateTime<Utc>) -> Result<TabChange, InvalidTabEvent> {
    let dto: TabEventDto =
        serde_json::from_slice(payload).map_err(|err| InvalidTabEvent::Malformed(err.to_string()))?;
    let browser = match dto.browser.as_str() {
        "chrome" => BrowserKind::Chrome,
        "edge" => BrowserKind::Edge,
        "firefox" => BrowserKind::Firefox,
        "brave" => BrowserKind::Brave,
        _ => return Err(InvalidTabEvent::UnknownBrowser(dto.browser)),
    };
    let url = dto
        .url
        .filter(|url| Url::parse(url).is_ok_and(|parsed| matches!(parsed.scheme(), "http" | "https")));
    Ok(TabChange {
        browser,
        url,
        title: dto.title.chars().take(MAX_TITLE_CHARS).collect(),
        at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at() -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }

    fn parse(json: &str) -> Result<TabChange, InvalidTabEvent> {
        parse_tab_event(json.as_bytes(), at())
    }

    #[test]
    fn keeps_the_raw_http_url_with_query_and_fragment() {
        let change = parse(r#"{"browser":"chrome","url":"https://example.com/a?b=1#c","title":"A"}"#).unwrap();

        assert_eq!(change.browser, BrowserKind::Chrome);
        assert_eq!(change.url.as_deref(), Some("https://example.com/a?b=1#c"));
        assert_eq!(change.title, "A");
        assert_eq!(change.at, at());
    }

    #[test]
    fn maps_every_known_browser() {
        for (wire, kind) in [
            ("chrome", BrowserKind::Chrome),
            ("edge", BrowserKind::Edge),
            ("firefox", BrowserKind::Firefox),
            ("brave", BrowserKind::Brave),
        ] {
            let change = parse(&format!(r#"{{"browser":"{wire}","url":null,"title":""}}"#)).unwrap();
            assert_eq!(change.browser, kind);
        }
    }

    #[test]
    fn rejects_an_unknown_browser() {
        assert_eq!(
            parse(r#"{"browser":"vivaldi","url":null,"title":""}"#).unwrap_err(),
            InvalidTabEvent::UnknownBrowser("vivaldi".into())
        );
    }

    #[test]
    fn non_web_pages_have_no_url() {
        for url in ["null", r#""chrome://newtab/""#, r#""file:///etc/hosts""#, r#""not a url""#] {
            let change = parse(&format!(r#"{{"browser":"firefox","url":{url},"title":"New Tab"}}"#)).unwrap();
            assert_eq!(change.url, None, "{url}");
        }
    }

    #[test]
    fn caps_the_title() {
        let title = "é".repeat(MAX_TITLE_CHARS + 10);
        let change = parse(&format!(r#"{{"browser":"edge","url":null,"title":"{title}"}}"#)).unwrap();

        assert_eq!(change.title.chars().count(), MAX_TITLE_CHARS);
    }

    #[test]
    fn rejects_malformed_payloads() {
        for payload in ["", "{}", "not json", r#"{"browser":"chrome","url":1,"title":""}"#] {
            assert!(matches!(parse(payload), Err(InvalidTabEvent::Malformed(_))), "{payload}");
        }
    }
}
