//! The dev server's own URLs, as macros: each literal is both matched per
//! request and `concat!`ed into the client script, and only a literal can be.

/// The source-opening endpoint's path.
macro_rules! open_endpoint {
    () => {
        "/__baudelaire/open"
    };
}

/// The live-reload endpoint's path.
macro_rules! live_endpoint {
    () => {
        "/__baudelaire/live"
    };
}
