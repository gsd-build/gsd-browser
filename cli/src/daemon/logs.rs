//! Log buffering infrastructure and CDP event listener spawners.
//!
//! Provides a generic `LogBuffer<T>` FIFO with a 1000-entry cap, plus
//! background tokio tasks that consume CDP event streams and push entries
//! into shared buffers.

use gsd_browser_common::types::{ConsoleLogEntry, DialogLogEntry, NetworkLogEntry};
use chromiumoxide::cdp::browser_protocol::network::{EventLoadingFailed, EventResponseReceived};
use chromiumoxide::cdp::browser_protocol::page::{
    EventJavascriptDialogOpening, HandleJavaScriptDialogParams,
};
use chromiumoxide::cdp::js_protocol::runtime::{EventConsoleApiCalled, EventExceptionThrown};
use chromiumoxide::Page;
use futures::StreamExt;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tracing::{debug, warn};

/// Maximum number of entries kept per buffer.
const MAX_BUFFER_SIZE: usize = 1000;

/// A thread-safe FIFO buffer with a fixed capacity.
/// Oldest entries are dropped when the buffer is full.
#[derive(Debug, Clone)]
pub struct LogBuffer<T> {
    inner: Arc<Mutex<VecDeque<T>>>,
}

impl<T: Clone> LogBuffer<T> {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(MAX_BUFFER_SIZE))),
        }
    }

    /// Push an entry, dropping the oldest if at capacity.
    pub fn push(&self, entry: T) {
        let mut buf = self.inner.lock().unwrap();
        if buf.len() >= MAX_BUFFER_SIZE {
            buf.pop_front();
        }
        buf.push_back(entry);
    }

    /// Drain all entries, clearing the buffer. Returns them in insertion order.
    pub fn drain(&self) -> Vec<T> {
        let mut buf = self.inner.lock().unwrap();
        buf.drain(..).collect()
    }

    /// Return a snapshot (clone) of all entries without clearing.
    pub fn snapshot(&self) -> Vec<T> {
        let buf = self.inner.lock().unwrap();
        buf.iter().cloned().collect()
    }

    /// Current number of entries in the buffer.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }
}

/// Holds all log buffers for the daemon.
pub struct DaemonLogs {
    pub console: LogBuffer<ConsoleLogEntry>,
    pub network: LogBuffer<NetworkLogEntry>,
    pub dialog: LogBuffer<DialogLogEntry>,
}

impl DaemonLogs {
    pub fn new() -> Self {
        Self {
            console: LogBuffer::new(),
            network: LogBuffer::new(),
            dialog: LogBuffer::new(),
        }
    }
}

/// Spawn a background task that listens for `Runtime.consoleAPICalled` events
/// and pushes them into the console log buffer.
pub async fn spawn_console_listener(page: &Page, buffer: LogBuffer<ConsoleLogEntry>) {
    let mut stream = match page.event_listener::<EventConsoleApiCalled>().await {
        Ok(s) => s,
        Err(e) => {
            warn!("spawn_console_listener: failed to create event listener: {e}");
            return;
        }
    };

    debug!("spawn_console_listener: listener spawned");

    tokio::spawn(async move {
        while let Some(event) = stream.next().await {
            // Concatenate all args into a single text string
            let text: String = event
                .args
                .iter()
                .filter_map(|arg| {
                    // Try .value first (primitive), then .description (objects)
                    arg.value
                        .as_ref()
                        .map(|v| {
                            if let Some(s) = v.as_str() {
                                s.to_string()
                            } else {
                                v.to_string()
                            }
                        })
                        .or_else(|| arg.description.clone())
                })
                .collect::<Vec<_>>()
                .join(" ");

            let log_type = event.r#type.as_ref().to_string();
            let timestamp = *event.timestamp.inner();

            let entry = ConsoleLogEntry {
                log_type,
                text,
                timestamp,
                url: String::new(), // consoleAPICalled doesn't include URL directly
            };
            buffer.push(entry);
        }
        warn!("spawn_console_listener: event stream closed");
    });
}

/// Spawn a background task that listens for `Runtime.exceptionThrown` events
/// and pushes them into the console log buffer as "pageerror" type.
pub async fn spawn_exception_listener(page: &Page, buffer: LogBuffer<ConsoleLogEntry>) {
    let mut stream = match page.event_listener::<EventExceptionThrown>().await {
        Ok(s) => s,
        Err(e) => {
            warn!("spawn_exception_listener: failed to create event listener: {e}");
            return;
        }
    };

    debug!("spawn_exception_listener: listener spawned");

    tokio::spawn(async move {
        while let Some(event) = stream.next().await {
            let details = &event.exception_details;
            let text = if let Some(exc) = &details.exception {
                exc.description
                    .clone()
                    .unwrap_or_else(|| details.text.clone())
            } else {
                details.text.clone()
            };
            let url = details.url.clone().unwrap_or_default();
            let timestamp = *event.timestamp.inner();

            let entry = ConsoleLogEntry {
                log_type: "pageerror".to_string(),
                text,
                timestamp,
                url,
            };
            buffer.push(entry);
        }
        warn!("spawn_exception_listener: event stream closed");
    });
}

/// Spawn background tasks that listen for `Network.responseReceived` and
/// `Network.loadingFailed` events.
pub async fn spawn_network_listener(page: &Page, buffer: LogBuffer<NetworkLogEntry>) {
    // Listener for successful responses
    let buffer_resp = buffer.clone();
    let mut resp_stream = match page.event_listener::<EventResponseReceived>().await {
        Ok(s) => s,
        Err(e) => {
            warn!("spawn_network_listener: failed to create responseReceived listener: {e}");
            return;
        }
    };

    debug!("spawn_network_listener: responseReceived listener spawned");

    tokio::spawn(async move {
        while let Some(event) = resp_stream.next().await {
            let response = &event.response;
            let status = response.status as u32;
            let resource_type = format!("{:?}", event.r#type);

            // Try to extract HTTP method from request_headers JSON blob
            let method = response
                .request_headers
                .as_ref()
                .and_then(|h| {
                    let val = h.inner();
                    val.get(":method")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| "GET".to_string());

            let entry = NetworkLogEntry {
                method,
                url: response.url.clone(),
                status,
                resource_type,
                timestamp: *event.timestamp.inner(),
                failed: status >= 400,
                failure_text: String::new(),
                response_body: String::new(),
            };
            buffer_resp.push(entry);
        }
        warn!("spawn_network_listener: responseReceived stream closed");
    });

    // Listener for failed requests
    let buffer_fail = buffer.clone();
    let mut fail_stream = match page.event_listener::<EventLoadingFailed>().await {
        Ok(s) => s,
        Err(e) => {
            warn!("spawn_network_listener: failed to create loadingFailed listener: {e}");
            return;
        }
    };

    debug!("spawn_network_listener: loadingFailed listener spawned");

    tokio::spawn(async move {
        while let Some(event) = fail_stream.next().await {
            let resource_type = format!("{:?}", event.r#type);

            let entry = NetworkLogEntry {
                method: String::new(),
                url: String::new(), // loadingFailed doesn't include URL, only requestId
                status: 0,
                resource_type,
                timestamp: *event.timestamp.inner(),
                failed: true,
                failure_text: event.error_text.clone(),
                response_body: String::new(),
            };
            buffer_fail.push(entry);
        }
        warn!("spawn_network_listener: loadingFailed stream closed");
    });
}

/// Spawn a background task that listens for `Page.javascriptDialogOpening`
/// events, buffers them, and auto-accepts each dialog.
pub async fn spawn_dialog_listener(page: &Page, buffer: LogBuffer<DialogLogEntry>) {
    let mut stream = match page.event_listener::<EventJavascriptDialogOpening>().await {
        Ok(s) => s,
        Err(e) => {
            warn!("spawn_dialog_listener: failed to create event listener: {e}");
            return;
        }
    };

    // We need a clone of the page handle to auto-dismiss dialogs
    let page_for_dismiss = page.clone();

    debug!("spawn_dialog_listener: listener spawned");

    tokio::spawn(async move {
        while let Some(event) = stream.next().await {
            let dialog_type = event.r#type.as_ref().to_string();
            let default_value = event.default_prompt.clone().unwrap_or_default();

            let entry = DialogLogEntry {
                dialog_type,
                message: event.message.clone(),
                timestamp: 0.0, // DialogOpening doesn't have a timestamp field
                url: event.url.clone(),
                default_value,
                accepted: true,
            };
            buffer.push(entry);

            // Auto-accept the dialog so the page doesn't hang
            if let Err(e) = page_for_dismiss
                .execute(HandleJavaScriptDialogParams::new(true))
                .await
            {
                warn!("spawn_dialog_listener: failed to auto-accept dialog: {e}");
            }
        }
        warn!("spawn_dialog_listener: event stream closed");
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gsd_browser_common::types::ConsoleLogEntry;

    #[test]
    fn log_buffer_push_and_drain() {
        let buf = LogBuffer::<ConsoleLogEntry>::new();
        buf.push(ConsoleLogEntry {
            log_type: "log".into(),
            text: "hello".into(),
            timestamp: 1.0,
            url: String::new(),
        });
        buf.push(ConsoleLogEntry {
            log_type: "error".into(),
            text: "oops".into(),
            timestamp: 2.0,
            url: String::new(),
        });
        let entries = buf.drain();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].text, "hello");
        assert_eq!(entries[1].text, "oops");
        // Buffer is empty after drain
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn log_buffer_snapshot_does_not_clear() {
        let buf = LogBuffer::<ConsoleLogEntry>::new();
        buf.push(ConsoleLogEntry {
            log_type: "log".into(),
            text: "test".into(),
            timestamp: 1.0,
            url: String::new(),
        });
        let snap = buf.snapshot();
        assert_eq!(snap.len(), 1);
        // Buffer still has the entry
        assert_eq!(buf.len(), 1);
    }

    #[test]
    fn log_buffer_cap_at_1000() {
        let buf = LogBuffer::<ConsoleLogEntry>::new();
        for i in 0..1050 {
            buf.push(ConsoleLogEntry {
                log_type: "log".into(),
                text: format!("entry-{i}"),
                timestamp: i as f64,
                url: String::new(),
            });
        }
        assert_eq!(buf.len(), MAX_BUFFER_SIZE);
        let entries = buf.drain();
        // Oldest 50 should be dropped, first remaining is entry-50
        assert_eq!(entries[0].text, "entry-50");
        assert_eq!(entries.last().unwrap().text, "entry-1049");
    }

    #[test]
    fn log_buffer_empty_drain_returns_empty() {
        let buf = LogBuffer::<ConsoleLogEntry>::new();
        let entries = buf.drain();
        assert!(entries.is_empty());
    }
}
