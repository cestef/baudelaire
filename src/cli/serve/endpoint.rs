//! The dev server's own URLs, as macros.
//!
//! Each literal is both matched per request and `concat!`ed into the client
//! script that calls it, and only a literal can be `concat!`ed, so the one
//! spelling has to be a macro rather than a `const`.

/// The source-opening endpoint's path, a macro for the same reason
/// [`live_endpoint`] is one: the literal is both matched per request and
/// `concat!`ed into the client that calls it.
macro_rules! open_endpoint {
    () => {
        "/__baudelaire/open"
    };
}

/// The live-reload endpoint's path, as a macro so the one literal serves both
/// [`Live::ENDPOINT`] (matched per request) and the client script that connects
/// to it: the script is a `const`, and only a literal can be `concat!`ed into
/// one. The two used to be separate literals that had to be kept equal by hand.
macro_rules! live_endpoint {
    () => {
        "/__baudelaire/live"
    };
}
