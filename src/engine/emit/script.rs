//! Assembly of the generated JavaScript clients: a prelude of build-time
//! constants, embedded sources from `js/`, and a tail that mounts or exports.

use std::fmt::Write;

use crate::codegen::{Js, Value};

/// A generated JavaScript client under construction.
pub(super) struct Script<'a> {
    prelude: String,
    parts: Vec<&'a str>,
}

impl<'a> Script<'a> {
    /// Open a script with the constants its sources close over, each written
    /// through the codegen escaper so a configured value carrying a quote
    /// cannot break out of its literal.
    pub(super) fn new(consts: &[(&str, &str)]) -> Self {
        Self::data(
            &consts
                .iter()
                .map(|(name, value)| (*name, Value::str(value)))
                .collect::<Vec<_>>(),
        )
    }

    /// The same, for constants holding a structured value: a map of index URLs,
    /// a block of configured defaults.
    pub(super) fn data(consts: &[(&str, Value)]) -> Self {
        let mut prelude = String::new();
        for (name, value) in consts {
            let _ = writeln!(prelude, "const {name} = {};", Js(value));
        }
        Self {
            prelude,
            parts: Vec::new(),
        }
    }

    /// Append one embedded source; parts share a single module scope.
    pub(super) fn part(mut self, source: &'a str) -> Self {
        self.parts.push(source);
        self
    }

    /// Finish with a call to `name`, guarded on there being a document so the
    /// same sources stay importable where there is no DOM.
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
