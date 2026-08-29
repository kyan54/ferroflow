//! Custom `tracing_subscriber::Layer` that feeds every `tracing` event
//! app-wide into the shared `core_manager::logs::LogBuffer` ring buffer,
//! tagged `source: "app"` -- the same buffer sing-box's child-process
//! stdout/stderr lines are fed into under `source: "core"` (see
//! `core_manager::logs::spawn_line_reader`, wired up from
//! `CoreManager::start`). Together these two feeds back the `logs_get`/
//! `logs_clear` Tauri commands (`commands::logs`).
//!
//! Installed alongside (not instead of) the existing `tracing_subscriber::fmt`
//! layer in `run()` -- stdout logging behavior is unchanged, this just taps
//! the same event stream into a second sink.

use std::sync::Arc;

use core_manager::logs::LogBuffer;
use shared_types::LogLevel;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

pub struct LogCaptureLayer {
    buffer: Arc<LogBuffer>,
}

impl LogCaptureLayer {
    pub fn new(buffer: Arc<LogBuffer>) -> Self {
        Self { buffer }
    }
}

fn map_level(level: &Level) -> LogLevel {
    match *level {
        Level::TRACE => LogLevel::Trace,
        Level::DEBUG => LogLevel::Debug,
        Level::INFO => LogLevel::Info,
        Level::WARN => LogLevel::Warn,
        Level::ERROR => LogLevel::Error,
    }
}

/// Extracts just the formatted `message` field off a `tracing::Event` --
/// good enough for display purposes (matches what `tracing_subscriber::fmt`
/// prints as the main line text); any other structured fields an event
/// carries are not captured here.
#[derive(Default)]
struct MessageVisitor {
    message: Option<String>,
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}"));
        }
    }
}

impl<S: Subscriber> Layer<S> for LogCaptureLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        let message = visitor.message.unwrap_or_default();
        let target = event.metadata().target().to_string();
        self.buffer.push(map_level(event.metadata().level()), "app", Some(target), message);
    }
}
