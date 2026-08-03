//! Assembly of the generated TypeScript declarations.
//!
//! The `.d.ts` counterpart of [`Script`](super::script::Script): a declaration
//! is a prelude of build-time types, one or more embedded fragments from
//! `asset/types/`, and a module block around the result. [`Dts`] is that shape,
//! so the indentation, the default-export binding, and the way a value's type
//! is written each exist once instead of once per module.

use std::fmt::{Display, Write};

use crate::codegen::{Ts, Value};

/// A TypeScript declaration under construction: the inside of one
/// `declare module` block.
pub(crate) struct Dts {
    body: String,
}

impl Dts {
    /// The binding a default export goes through. A declaration cannot say
    /// `export default <type>`, so the type is named first and the name is
    /// exported.
    const DEFAULT: &'static str = "data";

    pub(crate) fn new() -> Self {
        Self {
            body: String::new(),
        }
    }

    /// Append one embedded fragment: the hand-written half of a declaration,
    /// for the modules whose shape is fixed rather than read off site data.
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

    /// Everything `specifier` declares, for a module serving the same thing
    /// under another name. A declaration file has no other way to share a
    /// shape between modules.
    pub(crate) fn same_as(mut self, specifier: &str) -> Self {
        let _ = writeln!(self.body, "export * from \"{specifier}\";");
        self
    }

    /// The finished `declare module "<specifier>" { .. }` block, its body
    /// indented one level.
    pub(crate) fn module(&self, specifier: &str) -> String {
        let mut out = format!("declare module \"{specifier}\" {{\n");
        for line in self.body.lines() {
            match line.is_empty() {
                true => out.push('\n'),
                false => {
                    let _ = writeln!(out, "  {line}");
                }
            }
        }
        out.push_str("}\n");
        out
    }
}
