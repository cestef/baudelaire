//! Live reload: the event stream open tabs listen on.

use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use tiny_http::Request;

/// Live-reload coordination between the request handler and the rebuild loop.
///
/// The handler injects [`Live::SCRIPT`] into HTML responses; the injected
/// client opens a Server-Sent Events stream at [`Live::ENDPOINT`]. Each
/// successful rebuild calls [`Live::bump`], pushing a reload to every open
/// stream.
///
/// Streams are keyed by id so a closed connection is reaped promptly: the writer
/// thread wakes every [`Live::HEARTBEAT`] to send an SSE comment, notices the
/// dead socket on the failed write, and removes its own entry: no leak waiting
/// on the next rebuild.
#[derive(Clone, Default)]
pub(super) struct Live {
    /// One sender per open SSE connection, keyed for self-removal on close.
    streams: Arc<Mutex<HashMap<u64, flume::Sender<Signal>>>>,
    /// Monotonic source of stream ids.
    next_id: Arc<AtomicU64>,
}

impl Live {
    /// Endpoint the injected client connects to for the reload event stream.
    pub(super) const ENDPOINT: &'static str = live_endpoint!();

    /// How often an idle stream emits a keep-alive comment. Doubles as the upper
    /// bound on how long a closed connection lingers before it is reaped.
    const HEARTBEAT: Duration = Duration::from_secs(10);

    /// Client script appended to served HTML.
    ///
    /// One file per piece, composed here: the DOM helpers, the diagnostic
    /// renderer, the panel a message is shown in, the reload stream with its
    /// status dot, and the alt-click that opens a stamped element's source.
    /// Each is a lambda, so the block
    /// scope below is all they share and the endpoint literals reach them
    /// through the same `concat!` that keeps [`Live::ENDPOINT`] and
    /// [`Open::ENDPOINT`] in agreement with the client that calls them.
    pub(super) const SCRIPT: &'static str = concat!(
        "\n<script>\n{\n",
        "const dom = (",
        include_str!("../js/dom.js"),
        ")();\n",
        "const report = (",
        include_str!("../js/report.js"),
        ")(dom);\n",
        "const overlay = (",
        include_str!("../js/overlay.js"),
        ")(dom, report);\n",
        "(",
        include_str!("../js/live.js"),
        ")('",
        live_endpoint!(),
        "', overlay, dom);\n",
        "(",
        include_str!("../js/source.js"),
        ")('",
        open_endpoint!(),
        "', overlay);\n",
        "}\n</script>\n"
    );

    /// Raw HTTP response head that opens an SSE stream, plus a comment so the
    /// client registers the connection immediately.
    const HEAD: &'static str = "HTTP/1.1 200 OK\r\n\
        Content-Type: text/event-stream\r\n\
        Cache-Control: no-cache\r\n\
        Connection: keep-alive\r\n\
        \r\n\
        : ok\n\n";

    /// Advance every open stream, dropping any whose client has gone.
    pub(super) fn bump(&self) {
        self.push(&Signal::Reload);
    }

    /// Put a failed rebuild's diagnostic on screen in every open tab.
    ///
    /// The terminal already says this; the browser did not, and the browser is
    /// where the author is looking. `text` is the same rendered diagnostic,
    /// plain, carried as a JSON string so it survives SSE's line framing.
    pub(super) fn failed(&self, text: &str) {
        let payload = serde_json::to_string(text).unwrap_or_else(|_| String::from("\"\""));
        self.push(&Signal::Failed(payload));
    }

    fn push(&self, signal: &Signal) {
        self.streams
            .lock()
            .retain(|_, tx| tx.send(signal.clone()).is_ok());
    }

    /// Open an SSE stream for `req` on its own thread, writing directly to the
    /// socket so each event flushes the instant a rebuild finishes. The thread
    /// removes its own entry when it ends, so a closed tab frees its slot within
    /// one [`Live::HEARTBEAT`] instead of lingering until the next rebuild.
    pub(super) fn serve(&self, req: Request) {
        let (tx, signals) = flume::unbounded();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.streams.lock().insert(id, tx);
        let streams = Arc::clone(&self.streams);
        std::thread::spawn(move || {
            let mut socket = req.into_writer();
            if socket.write_all(Self::HEAD.as_bytes()).is_ok() && socket.flush().is_ok() {
                loop {
                    // A rebuild pushes `reload`; an idle timeout emits a comment
                    // keep-alive whose failed write reveals a closed socket.
                    let payload = match signals.recv_timeout(Self::HEARTBEAT) {
                        Ok(signal) => signal.frame(),
                        Err(flume::RecvTimeoutError::Timeout) => ": ping\n\n".to_owned(),
                        Err(flume::RecvTimeoutError::Disconnected) => break,
                    };
                    if socket.write_all(payload.as_bytes()).is_err() || socket.flush().is_err() {
                        break;
                    }
                }
            }
            streams.lock().remove(&id);
        });
    }
}

/// What a rebuild pushes down an open live-reload stream.
#[derive(Debug, Clone)]
pub(super) enum Signal {
    /// The rebuild succeeded: reload the page.
    Reload,
    /// It did not, carrying the rendered diagnostic as a JSON string.
    Failed(String),
}

impl Signal {
    /// This signal as an SSE frame. The default (unnamed) event stays `reload`,
    /// so a client from before the overlay existed still reloads.
    fn frame(&self) -> String {
        match self {
            Self::Reload => "data: reload\n\n".to_owned(),
            Self::Failed(json) => format!("event: failed\ndata: {json}\n\n"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stream whose client is gone is reaped on the next bump, rather than
    /// accumulating in the registry.
    #[test]
    fn bump_reaps_streams_whose_client_disconnected() {
        let live = Live::default();
        let (live_tx, live_rx) = flume::unbounded();
        let (dead_tx, dead_rx) = flume::unbounded::<Signal>();
        live.streams.lock().insert(0, live_tx);
        live.streams.lock().insert(1, dead_tx);
        // The dead stream's receiver (its writer thread) is gone.
        drop(dead_rx);

        live.bump();

        let streams = live.streams.lock();
        assert!(streams.contains_key(&0), "live stream kept");
        assert!(!streams.contains_key(&1), "disconnected stream reaped");
        // The surviving stream received the reload signal.
        assert!(matches!(live_rx.try_recv(), Ok(Signal::Reload)));
    }
    /// A failed rebuild reaches the browser too. It used to reach only the
    /// terminal, so a tab kept showing the last good page with no hint that the
    /// save had not taken.
    #[test]
    fn a_failed_rebuild_pushes_its_diagnostic_to_open_tabs() {
        let live = Live::default();
        let (tx, rx) = flume::unbounded();
        live.streams.lock().insert(0, tx);

        live.failed("expected `}`\n  at line 3");

        let Ok(signal) = rx.try_recv() else {
            panic!("the open stream should have been signalled");
        };
        let frame = signal.frame();
        // A named event, so it is distinguishable from `EventSource`'s own
        // transport errors, and JSON-encoded so the newline survives SSE's
        // line framing intact.
        assert!(frame.starts_with("event: failed\ndata: "), "{frame}");
        assert!(frame.contains(r"expected `}`\n  at line 3"), "{frame}");
        assert!(frame.ends_with("\n\n"), "{frame}");
        // Exactly one `data:` line: an unencoded newline would split the frame
        // and the client would parse half a diagnostic.
        assert_eq!(frame.matches("data: ").count(), 1, "{frame}");
    }
}
