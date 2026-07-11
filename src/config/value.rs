//! Typed accessors over [`KdlValue`] and environment-variable expansion for
//! config strings. The single place a raw KDL value becomes a config field.

use kdl::KdlValue;
use miette::SourceSpan;

use crate::config::SortKey;
use crate::config::dispatch::Keys;
use crate::error::{ConfigError, Result};

/// Expands `${VAR}` references in config string values from the process
/// environment, with an optional `${VAR:-default}` fallback. An unset variable
/// with no default expands to the empty string. The single place the config
/// surface reads the environment, so the rule is uniform across every string.
struct Env;

impl Env {
    fn expand(raw: &str) -> String {
        Self::expand_with(raw, |name| std::env::var(name).ok())
    }

    /// Expansion against an arbitrary variable lookup — the core logic, kept
    /// free of the process environment so it is testable without mutating global
    /// state (unsound under multi-threaded test runners).
    fn expand_with(raw: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
        let mut out = String::with_capacity(raw.len());
        let mut rest = raw;
        while let Some(start) = rest.find("${") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            let Some(end) = after.find('}') else {
                // No closing brace: emit the rest verbatim and stop.
                out.push_str(&rest[start..]);
                return out;
            };
            let (name, default) = match after[..end].split_once(":-") {
                Some((name, default)) => (name.trim(), Some(default)),
                None => (after[..end].trim(), None),
            };
            let value = lookup(name)
                .or_else(|| default.map(str::to_owned))
                .unwrap_or_default();
            out.push_str(&value);
            rest = &after[end + 1..];
        }
        out.push_str(rest);
        out
    }
}

pub(super) trait ValueExt {
    fn as_str(&self, text: &str, span: SourceSpan) -> Result<String>;
    fn integer(&self, text: &str, span: SourceSpan) -> Result<i64>;
    fn is_true(&self) -> bool;
    fn kind(&self) -> &'static str;
    fn sort(&self, text: &str, span: SourceSpan) -> Result<SortKey>;
    /// Map a string value through a `(name, value)` table, erroring on an
    /// unknown name with a nearest-match hint — the single-value counterpart of
    /// `NodeExt::mapped`, so the table drives both parsing and error help.
    fn one<T: Copy>(
        &self,
        text: &str,
        span: SourceSpan,
        table: &[(&'static str, T)],
    ) -> Result<T>;
}

impl ValueExt for KdlValue {
    fn as_str(&self, text: &str, span: SourceSpan) -> Result<String> {
        match self.as_string() {
            Some(s) => Ok(Env::expand(s)),
            None => Err(ConfigError::bad_value(
                text,
                format!("expected string, got {}", self.kind()),
                span,
            )
            .into()),
        }
    }

    fn integer(&self, text: &str, span: SourceSpan) -> Result<i64> {
        match self.as_integer() {
            Some(n) => Ok(n as i64),
            None => Err(ConfigError::bad_value(
                text,
                format!("expected integer, got {}", self.kind()),
                span,
            )
            .into()),
        }
    }

    fn is_true(&self) -> bool {
        self.as_bool() == Some(true)
    }

    fn kind(&self) -> &'static str {
        match self {
            KdlValue::String(_) => "string",
            KdlValue::Integer(_) => "integer",
            KdlValue::Float(_) => "float",
            KdlValue::Bool(_) => "bool",
            KdlValue::Null => "null",
        }
    }

    fn sort(&self, text: &str, span: SourceSpan) -> Result<SortKey> {
        self.one(text, span, &[
            ("order", SortKey::Order),
            ("date", SortKey::Date),
            ("title", SortKey::Title),
        ])
    }

    fn one<T: Copy>(
        &self,
        text: &str,
        span: SourceSpan,
        table: &[(&'static str, T)],
    ) -> Result<T> {
        let name = self.as_str(text, span)?;
        table
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, value)| *value)
            .ok_or_else(|| Keys::unknown(table, text, &name, span))
    }
}

#[cfg(test)]
mod tests {
    use super::Env;

    #[test]
    fn env_expands_variables_defaults_and_literals() {
        // A fixed lookup — no real environment touched, so this is sound under
        // any test runner.
        let env = |name: &str| (name == "APP_ENV").then(|| "prod".to_owned());
        assert_eq!(Env::expand_with("site-${APP_ENV}", env), "site-prod");
        assert_eq!(Env::expand_with("${MISSING:-fallback}", env), "fallback");
        assert_eq!(Env::expand_with("${MISSING}", env), "");
        assert_eq!(Env::expand_with("no vars here", env), "no vars here");
        // An unterminated reference is left verbatim.
        assert_eq!(Env::expand_with("half ${OPEN", env), "half ${OPEN");
    }
}
