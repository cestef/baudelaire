//! Assembly of the generated TypeScript declarations: build-time types,
//! embedded fragments from `asset/types/`, and a module block around them.

use std::fmt::{Display, Write};

use crate::codegen::{Ts, Value};

/// A TypeScript declaration under construction: the inside of one
/// `declare module` block.
pub(crate) struct Dts {
    body: String,
}

impl Dts {
    /// The binding a default export goes through, since a declaration cannot
    /// say `export default <type>`.
    const DEFAULT: &'static str = "data";

    pub(crate) fn new() -> Self {
        Self {
            body: String::new(),
        }
    }

    /// Append one embedded fragment: the hand-written half of a declaration,
    /// for a module whose shape is fixed rather than read off site data.
    pub(crate) fn part(mut self, fragment: &str) -> Self {
        if !self.body.is_empty() {
            self.body.push('\n');
        }
        self.body.push_str(fragment);
        self
    }

    /// A named type, for a fragment to refer to: `export type <name> = <ty>;`.
    pub(crate) fn alias(mut self, name: &str, ty: impl Display) -> Self {
        let _ = writeln!(self.body, "export type {name} = {ty};");
        self
    }

    /// A named export, typed from the value the module serves under it.
    pub(crate) fn constant(mut self, name: &str, value: &Value) -> Self {
        let _ = writeln!(self.body, "export const {name}: {};", Ts(value));
        self
    }

    /// The default export, typed from the value the module serves.
    pub(crate) fn default(mut self, value: &Value) -> Self {
        let name = Self::DEFAULT;
        let _ = writeln!(self.body, "const {name}: {};", Ts(value));
        let _ = writeln!(self.body, "export default {name};");
        self
    }

    /// The finished `declare module "<specifier>" { .. }` block, its body
    /// indented one level.
    pub(crate) fn module(&self, specifier: &str) -> String {
        let mut out = format!("declare module \"{specifier}\" {{\n");
        for line in self.body.lines() {
            if line.is_empty() {
                out.push('\n');
            } else {
                let _ = writeln!(out, "  {line}");
            }
        }
        out.push_str("}\n");
        out
    }
}
