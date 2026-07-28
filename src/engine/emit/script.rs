//! Assembly of the generated JavaScript clients.
//!
//! Every client this crate emits (the SPA runtime, the search client, the
//! single-file export's inline router) is the same shape: a prelude of
//! build-time constants, one or more embedded sources from `js/`, and a tail
//! that either mounts the thing or exports it. [`Script`] is that shape, so the
//! prelude escaping, the part separator, the auto-mount guard, and the export
//! list each exist once instead of once per emitter.

use std::fmt::Write;

use crate::codegen::{Js, Value};

/// A generated JavaScript client under construction.
pub(super) struct Script<'a> {
    prelude: String,
    parts: Vec<&'a str>,
}

impl<'a> Script<'a> {
    /// Open a script with the constants its sources close over.
    ///
    /// Each value is written through the codegen escaper rather than
    /// interpolated, so a configured container selector or index URL carrying a
    /// quote cannot break out of its literal.
    pub(super) fn new(consts: &[(&str, &str)]) -> Self {
        let mut prelude = String::new();
        for (name, value) in consts {
            let _ = writeln!(prelude, "const {name} = {};", Js(&Value::str(value)));
        }
        Self {
            prelude,
            parts: Vec::new(),
        }
    }

    /// Append one embedded source. Parts share a single module scope, which is
    /// how an engine's helpers are visible to the UI concatenated after it.
    pub(super) fn part(mut self, source: &'a str) -> Self {
        self.parts.push(source);
        self
    }

    /// Finish with a call to `name`, guarded on there being a document: that is
    /// what makes the emitted file work by dropping one `<script>` in, while the
    /// same sources stay importable somewhere without a DOM.
    pub(super) fn mount(self, name: &str) -> String {
        self.tail(&format!("if (typeof document !== \"undefined\") {name}();"))
    }

    /// Finish with a named export list, for the virtual-module build where the
    /// importer decides when to mount.
    #[cfg(feature = "js")]
    pub(super) fn exports(self, names: &[&str]) -> String {
        self.tail(&format!("export {{ {} }};", names.join(", ")))
    }

    /// Finish with nothing after the last part.
    pub(super) fn finish(self) -> String {
        self.prelude + &self.parts.join("\n")
    }

    /// Finish with one trailing statement, on its own line.
    fn tail(self, statement: &str) -> String {
        format!("{}\n{statement}\n", self.finish())
    }
}
