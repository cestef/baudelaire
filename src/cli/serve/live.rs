//! Live reload: the event stream open tabs listen on.

use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use tiny_http::Request;

/// Live-reload coordination between the request handler and the rebuild loop:
/// the handler injects [`Live::SCRIPT`], the injected client opens a
/// Server-Sent Events stream at [`Live::ENDPOINT`], and each successful rebuild
/// calls [`Live::bump`].
#[derive(Clone, Default)]
pub(super) struct Live {
    /// One sender per open SSE connection, keyed for self-removal on close.
    streams: Arc<Mutex<HashMap<u64, flume::Sender<Signal>>>>,
    next_id: Arc<AtomicU64>,
}

impl Live {
    /// Endpoint the injected client connects to for the reload event stream.
    pub(super) const ENDPOINT: &'static str = live_endpoint!();

    /// How often an idle stream emits a keep-alive comment. Doubles as the upper
    /// bound on how long a closed connection lingers before it is reaped.
    const HEARTBEAT: Duration = Duration::from_secs(10);

    /// Client script appended to served HTML, one lambda per file, sharing only
    /// the block scope below.
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

    /// Tell every open tab to fetch its stylesheets again, leaving the page as
    /// the reader left it: their scroll, their open menu, their focus.
    pub(super) fn restyle(&self) {
        self.push(&Signal::Styles);
    }

    /// Put a failed rebuild's diagnostic on screen in every open tab; `text` is
    /// carried as a JSON string so it survives SSE's line framing.
    pub(super) fn failed(&self, text: &str) {
        let payload = serde_json::to_string(text).unwrap_or_else(|_| String::from("\"\""));
        self.push(&Signal::Failed(payload));
    }

    fn push(&self, signal: &Signal) {
        self.streams
            .lock()
            .retain(|_, tx| tx.send(signal.clone()).is_ok());
    }

    /// Open an SSE stream for `req` on its own thread, writing straight to the
    /// socket so each event flushes at once. The thread removes its own entry
    /// when it ends, within one [`Live::HEARTBEAT`] of the tab closing.
    pub(super) fn serve(&self, req: Request) {
        let (tx, signals) = flume::unbounded();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.streams.lock().insert(id, tx);
        let streams = Arc::clone(&self.streams);
        std::thread::spawn(move || {
            let mut socket = req.into_writer();
            if socket.write_all(Self::HEAD.as_bytes()).is_ok() && socket.flush().is_ok() {
                loop {
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
    /// It changed stylesheets and nothing else: swap them in place.
    Styles,
    /// It did not, carrying the rendered diagnostic as a JSON string.
    Failed(String),
}

impl Signal {
    /// This signal as an SSE frame; the unnamed default event stays `reload`.
    fn frame(&self) -> String {
        match self {
            Self::Reload => "data: reload\n\n".to_owned(),
            Self::Styles => "data: styles\n\n".to_owned(),
            Self::Failed(json) => format!("event: failed\ndata: {json}\n\n"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_reaps_streams_whose_client_disconnected() {
        let live = Live::default();
        let (live_tx, live_rx) = flume::unbounded();
        let (dead_tx, dead_rx) = flume::unbounded::<Signal>();
        live.streams.lock().insert(0, live_tx);
        live.streams.lock().insert(1, dead_tx);
        drop(dead_rx);

        live.bump();

        let streams = live.streams.lock();
        assert!(streams.contains_key(&0), "live stream kept");
        assert!(!streams.contains_key(&1), "disconnected stream reaped");
        assert!(matches!(live_rx.try_recv(), Ok(Signal::Reload)));
    }

    /// The two success signals are one event with two payloads, since the
    /// client tells them apart by what it reads rather than by what it listens
    /// for.
    #[test]
    fn a_restyle_is_the_default_event_carrying_styles() {
        let live = Live::default();
        let (tx, rx) = flume::unbounded();
        live.streams.lock().insert(0, tx);

        live.restyle();

        let Ok(signal) = rx.try_recv() else {
            panic!("the open stream should have been signalled");
        };
        assert_eq!(signal.frame(), "data: styles\n\n");
        assert!(matches!(signal, Signal::Styles));
    }

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
        assert!(frame.starts_with("event: failed\ndata: "), "{frame}");
        assert!(frame.contains(r"expected `}`\n  at line 3"), "{frame}");
        assert!(frame.ends_with("\n\n"), "{frame}");
        assert_eq!(frame.matches("data: ").count(), 1, "{frame}");
    }
}
