use std::fmt;
use std::io::Write;

use chrono::{SecondsFormat, Utc};
use serde_json::{Map, Value};
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::{Context, Layer};

use super::{area_for, LogEntry, LogLevel};

/// Writes every event it is given as one [`LogEntry`] JSON line. Failures to write are dropped
/// silently: reporting them would mean logging from inside the logger.
pub struct AppLogLayer<W> {
    make_writer: W,
}

impl<W> AppLogLayer<W> {
    pub fn new(make_writer: W) -> Self {
        Self { make_writer }
    }
}

impl<S, W> Layer<S> for AppLogLayer<W>
where
    S: Subscriber,
    W: for<'writer> MakeWriter<'writer> + 'static,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);
        let entry = LogEntry {
            ts: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            level: LogLevel::from(metadata.level()),
            target: metadata.target().to_string(),
            area: area_for(metadata.target()),
            message: visitor.message,
            fields: visitor.fields,
        };
        let Ok(mut line) = serde_json::to_vec(&entry) else {
            return;
        };
        line.push(b'\n');
        let _ = self.make_writer.make_writer_for(metadata).write_all(&line);
    }
}

#[derive(Default)]
struct FieldVisitor {
    message: String,
    fields: Map<String, Value>,
}

impl FieldVisitor {
    fn insert(&mut self, field: &Field, value: Value) {
        if field.name().starts_with("log.") {
            return;
        }
        match (field.name(), value) {
            ("message", Value::String(text)) => self.message = text,
            (name, value) => {
                self.fields.insert(name.to_string(), value);
            }
        }
    }
}

impl Visit for FieldVisitor {
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.insert(field, Value::from(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.insert(field, Value::from(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.insert(field, Value::from(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.insert(field, Value::from(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.insert(field, Value::from(value));
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.insert(field, Value::from(value.to_string()));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.insert(field, Value::from(format!("{value:?}")));
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::{Arc, Mutex};

    use tracing_subscriber::layer::SubscriberExt;

    use super::*;
    use crate::logs::LogArea;

    #[derive(Clone, Default)]
    pub(crate) struct Buffer(Arc<Mutex<Vec<u8>>>);

    impl Buffer {
        pub(crate) fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    impl Write for Buffer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Buffer {
        type Writer = Buffer;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    pub(crate) fn capture(emit: impl FnOnce()) -> String {
        let buffer = Buffer::default();
        let subscriber = tracing_subscriber::registry().with(AppLogLayer::new(buffer.clone()));
        tracing::subscriber::with_default(subscriber, emit);
        buffer.text()
    }

    #[test]
    fn writes_one_json_line_per_event_with_typed_fields() {
        let text = capture(|| {
            let host = "api.openai.com";
            tracing::info!(target: "makora_lib::network", %host, status = 200u16, bytes_sent = 512u64, ok = true, "HTTP request");
            tracing::warn!(error = ?"boom", "something failed");
        });
        let lines: Vec<LogEntry> = text.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
        assert_eq!(lines.len(), 2);

        let network = &lines[0];
        assert_eq!(network.level, LogLevel::Info);
        assert_eq!(network.area, LogArea::Network);
        assert_eq!(network.message, "HTTP request");
        assert_eq!(network.fields["host"], "api.openai.com");
        assert_eq!(network.fields["status"], 200);
        assert_eq!(network.fields["bytes_sent"], 512);
        assert_eq!(network.fields["ok"], true);
        assert!(!network.fields.contains_key("message"));

        assert_eq!(lines[1].level, LogLevel::Warn);
        assert_eq!(lines[1].fields["error"], "\"boom\"");
        assert!(lines[1].target.starts_with("makora_lib::logs"));
    }
}
