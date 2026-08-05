//! Typed accessors over [`KdlValue`] and environment-variable expansion for
//! config strings. The single place a raw KDL value becomes a config field.

use std::cell::Cell;
use std::fmt::{self, Display, Write as _};

use kdl::KdlValue;
use miette::SourceSpan;

use crate::config::dispatch::Keys;
use crate::config::{FieldType, Named};
use crate::error::{ConfigError, Result};

/// A `${VAR}` reference whose variable is unset and which carries no
/// `:-default`: the one failure mode of [`Env`] expansion.
#[derive(Debug)]
struct MissingVar(String);

/// A [`KdlValue`] written back as the KDL source that parses to it: the adapter
/// a diagnostic echoes an author's own value through, and the only correct one
/// where the message is a *line to write*.
///
/// `KdlValue`'s own `Display` writes a string bare wherever KDL's grammar would
/// take it as an identifier, so a help offering `drafts { suffix .x }` hands
/// back a line whose value has a different shape than the one written -- and
/// for a value carrying a space, a quote or an `=`, a line that does not parse
/// at all. A help whose whole promise is "write this instead" cannot be a line
/// that fails, so the quoting is not optional and the escaping is this
/// adapter's.
///
/// The [`Typst`](crate::codegen::Typst) and [`Js`](crate::codegen::Js) adapters
/// are the same shape for the same reason: a value crossing into another
/// language is written by a type that knows that language's quoting, never by
/// `to_string`.
pub(super) struct Kdl<'a>(pub(super) &'a KdlValue);

impl Display for Kdl<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(string) = self.0.as_string() else {
            // Every other scalar has exactly one spelling, and KDL's own
            // `Display` writes it: `#false`, `3`, `1.5`, `#null`.
            return write!(f, "{}", self.0);
        };
        f.write_char('"')?;
        for c in string.chars() {
            match c {
                '"' | '\\' => write!(f, "\\{c}")?,
                '\n' => f.write_str("\\n")?,
                '\r' => f.write_str("\\r")?,
                '\t' => f.write_str("\\t")?,
                _ => f.write_char(c)?,
            }
        }
        f.write_char('"')
    }
}

/// What an unset `${VAR}` with no `:-default` resolves to, which is not the
/// same question in both passes that read a config.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Unset {
    /// Every read whose value is going to be used: an unset variable is an
    /// error rather than an empty string, so a missing secret fails the build
    /// instead of shipping a blank value.
    Fails,
    /// A [`Structural`] pass: the reference stands in as
    /// [`PLACEHOLDER`](Unset::PLACEHOLDER), because the pass is checking which
    /// keys are written and not what they say.
    Stands,
}

impl Unset {
    /// What an unset variable stands in as during a structural pass.
    ///
    /// A syntactically complete https URL, and a legal relative path: the keys
    /// most often written as a bare `${VAR}` are exactly the ones carrying a
    /// shape check of their own (`deploy { s3 { endpoint } }` demands https, a
    /// generated file name has to stay under `dist`), and a pass checking
    /// structure must not fail on a value it invented itself.
    const PLACEHOLDER: &'static str = "https://example.invalid";

    /// The value an unset reference takes, or `None` when there is none and the
    /// reference is an error.
    fn stand_in(self) -> Option<String> {
        match self {
            Self::Fails => None,
            Self::Stands => Some(Self::PLACEHOLDER.to_owned()),
        }
    }
}

thread_local! {
    /// Whether this thread is inside a [`Structural`] pass. A thread-local
    /// rather than a parameter because the two passes differ at exactly one
    /// call site and agree at every other, while the value is read under a
    /// hundred handler signatures that have no business knowing which pass they
    /// are in.
    static STRUCTURAL: Cell<bool> = const { Cell::new(false) };
}

/// A pass that reads a config for its *shape* rather than its content: for as
/// long as the guard is alive, an unset `${VAR}` with no `:-default` stands in
/// as a placeholder instead of failing.
///
/// [`Config::check`](crate::config::Config) applies every profile -- selected or
/// not -- to a throwaway clone, so a typo in a profile nobody asked for is
/// caught by the parse that read it. That is a check of *structure*. A
/// profile's values belong to the environment of the build that selects it:
/// `profiles { prod { deploy { s3 { bucket "${S3_BUCKET}" } } } }` is the
/// documented shape, and reading it as a value made every local `build` fail
/// for want of a production secret.
///
/// Selecting a profile for real is the other path and is left exactly as it
/// was: `Config::with_profile` runs outside this guard, the value is about to
/// be used, and an unset variable is still a hard error. Nothing reaches a
/// deploy with a placeholder in it.
pub(super) struct Structural(bool);

impl Structural {
    /// Begin a structural pass on the current thread. The previous mode is
    /// restored when the guard drops, so an error raised inside one cannot
    /// leave a later real read resolving against a placeholder.
    ///
    /// The guard has to be bound: dropped where it is made, it would turn the
    /// mode on and off again before a single value was read.
    #[must_use]
    pub(super) fn begin() -> Self {
        Self(STRUCTURAL.replace(true))
    }

    /// How an unset variable resolves on this thread right now.
    fn mode() -> Unset {
        match STRUCTURAL.get() {
            true => Unset::Stands,
            false => Unset::Fails,
        }
    }
}

impl Drop for Structural {
    fn drop(&mut self) {
        STRUCTURAL.set(self.0);
    }
}

/// Expands `${VAR}` references in config string values from the process
/// environment, with an optional `${VAR:-default}` fallback. An unset variable
/// with no default is an error ([`MissingVar`]) rather than a silent empty
/// string. The single place the config surface reads the environment, so the
/// rule is uniform across every string.
struct Env;

impl Env {
    fn expand(raw: &str) -> Result<String, MissingVar> {
        Self::expand_with(raw, |name| std::env::var(name).ok(), Structural::mode())
    }

    /// Expansion against an arbitrary variable lookup: the core logic, kept
    /// free of the process environment so it is testable without mutating global
    /// state (unsound under multi-threaded test runners). `unset` is passed in
    /// for the same reason.
    fn expand_with(
        raw: &str,
        lookup: impl Fn(&str) -> Option<String>,
        unset: Unset,
    ) -> Result<String, MissingVar> {
        let mut out = String::with_capacity(raw.len());
        let mut rest = raw;
        while let Some(start) = rest.find("${") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            let Some(end) = after.find('}') else {
                // no closing brace: not a reference, emit verbatim
                out.push_str(&rest[start..]);
                return Ok(out);
            };
            let (name, default) = match after[..end].split_once(":-") {
                Some((name, default)) => (name.trim(), Some(default)),
                None => (after[..end].trim(), None),
            };
            // The `:-default` the author wrote outranks any stand-in: it is a
            // real value, and a structural pass should see the one that a
            // selected build would.
            let value = lookup(name)
                .or_else(|| default.map(str::to_owned))
                .or_else(|| unset.stand_in())
                .ok_or_else(|| MissingVar(name.to_owned()))?;
            out.push_str(&value);
            rest = &after[end + 1..];
        }
        out.push_str(rest);
        Ok(out)
    }
}

pub(super) trait ValueExt {
    fn as_str(&self, text: &str, span: SourceSpan) -> Result<String>;
    fn integer(&self, text: &str, span: SourceSpan) -> Result<i64>;
    /// An integer required to fall within `min..=max`, erroring (never clamping)
    /// when it does not: the same policy as `port` and `paginate`.
    fn ranged(&self, text: &str, span: SourceSpan, min: i64, max: i64) -> Result<i64>;
    /// The [`ValueExt::ranged`] case where the bounds are already values of the
    /// narrow field type. The range check *is* the narrowing check, so the
    /// conversion back to `T` cannot fail and no site has to spell out a cast
    /// whose safety argument lives one line above it.
    fn bounded<T>(&self, text: &str, span: SourceSpan, min: T, max: T) -> Result<T>
    where
        T: TryFrom<i64> + Into<i64> + Copy;
    fn boolean(&self, text: &str, span: SourceSpan) -> Result<bool>;
    fn kind(&self) -> &'static str;
    /// Read a string value as one of `T`'s configured names, erroring on an
    /// unknown one with a nearest-match hint: the single-value counterpart of
    /// `NodeExt::mapped`, so [`Named::NAMES`] drives both parsing and error help.
    fn one<T: Named>(&self, text: &str, span: SourceSpan) -> Result<T>;
    /// Read a string value as a schema field's type expression (`list<dict>`).
    /// Its own reader and not [`ValueExt::one`] because a type is a shape rather
    /// than a name: the set it is drawn from is infinite.
    fn ty(&self, text: &str, span: SourceSpan) -> Result<FieldType>;
    /// Any KDL scalar as a [`codegen::Value`], for build-time constants passed
    /// straight through to client JS (`baudelaire:config`). Strings expand
    /// `${VAR}` like every other config string; a non-finite float is an error.
    ///
    /// [`codegen::Value`]: crate::codegen::Value
    fn scalar(&self, text: &str, span: SourceSpan) -> Result<crate::codegen::Value>;
}

impl ValueExt for KdlValue {
    fn as_str(&self, text: &str, span: SourceSpan) -> Result<String> {
        match self.as_string() {
            Some(s) => Env::expand(s)
                .map_err(|MissingVar(name)| ConfigError::env(text, &name, span).into()),
            None => Err(ConfigError::type_mismatch(text, "string", self.kind(), span).into()),
        }
    }

    fn integer(&self, text: &str, span: SourceSpan) -> Result<i64> {
        // kdl 6 integers are i128: a literal beyond i64 must not wrap.
        match self.as_integer() {
            Some(n) => {
                i64::try_from(n).map_err(|_| ConfigError::integer_overflow(text, n, span).into())
            }
            None => Err(ConfigError::type_mismatch(text, "integer", self.kind(), span).into()),
        }
    }

    fn ranged(&self, text: &str, span: SourceSpan, min: i64, max: i64) -> Result<i64> {
        let n = self.integer(text, span)?;
        if (min..=max).contains(&n) {
            Ok(n)
        } else {
            Err(ConfigError::out_of_range(text, min, max, n, span).into())
        }
    }

    fn bounded<T>(&self, text: &str, span: SourceSpan, min: T, max: T) -> Result<T>
    where
        T: TryFrom<i64> + Into<i64> + Copy,
    {
        let n = self.ranged(text, span, min.into(), max.into())?;
        let Ok(narrowed) = T::try_from(n) else {
            unreachable!("`ranged` already bounded the value to `T`'s own min..=max")
        };
        Ok(narrowed)
    }

    fn boolean(&self, text: &str, span: SourceSpan) -> Result<bool> {
        match self.as_bool() {
            Some(b) => Ok(b),
            None => Err(ConfigError::type_mismatch(text, "boolean", self.kind(), span).into()),
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::String(_) => "string",
            Self::Integer(_) => "integer",
            Self::Float(_) => "float",
            Self::Bool(_) => "boolean",
            Self::Null => "null",
        }
    }

    fn scalar(&self, text: &str, span: SourceSpan) -> Result<crate::codegen::Value> {
        use crate::codegen::Value;
        Ok(match self {
            Self::String(_) => Value::str(self.as_str(text, span)?),
            Self::Integer(_) => Value::Int(self.integer(text, span)?),
            Self::Float(f) if f.is_finite() => Value::Float(*f),
            Self::Float(_) => {
                return Err(
                    ConfigError::type_mismatch(text, "finite number", "float", span).into(),
                );
            }
            Self::Bool(b) => Value::Bool(*b),
            Self::Null => Value::None,
        })
    }

    fn one<T: Named>(&self, text: &str, span: SourceSpan) -> Result<T> {
        let name = self.as_str(text, span)?;
        T::of(&name).ok_or_else(|| Keys::unknown_value(T::NAMES, text, &name, span))
    }

    fn ty(&self, text: &str, span: SourceSpan) -> Result<FieldType> {
        let src = self.as_str(text, span)?;
        FieldType::parse(&src).map_err(|e| e.at(text, span))
    }
}

#[cfg(test)]
mod tests {
    use super::{Env, Kdl, Structural, Unset};
    use kdl::KdlValue;

    /// A help that says "write this instead" is copied, so what it prints has to
    /// parse. KDL's own `Display` writes a string bare wherever the grammar
    /// takes an identifier, which turns `suffix ".x"` into `suffix .x` and a
    /// value with a space into no line at all.
    #[test]
    fn a_value_is_written_back_as_the_kdl_that_parses_to_it() {
        let written = |value: KdlValue| Kdl(&value).to_string();
        let string = |s: &str| KdlValue::String(s.to_owned());
        assert_eq!(written(string(".x")), r#"".x""#, "a bare-looking string");
        assert_eq!(written(string("a b")), r#""a b""#, "a space");
        assert_eq!(written(string("say \"hi\"")), r#""say \"hi\"""#, "a quote");
        assert_eq!(written(string("c:\\x")), r#""c:\\x""#, "a backslash");
        assert_eq!(written(string("one\ntwo")), r#""one\ntwo""#, "a newline");
        // Every other scalar has one spelling, and KDL already writes it.
        assert_eq!(written(KdlValue::Bool(true)), "#true");
        assert_eq!(written(KdlValue::Integer(3)), "3");
        assert_eq!(written(KdlValue::Null), "#null");
    }

    #[test]
    fn env_expands_variables_defaults_and_literals() {
        // fixed lookup, no real env touched: sound under any test runner
        let env = |name: &str| (name == "APP_ENV").then(|| "prod".to_owned());
        let fails = Unset::Fails;
        assert_eq!(
            Env::expand_with("site-${APP_ENV}", env, fails).unwrap(),
            "site-prod"
        );
        assert_eq!(
            Env::expand_with("${MISSING:-fallback}", env, fails).unwrap(),
            "fallback"
        );
        assert_eq!(
            Env::expand_with("no vars here", env, fails).unwrap(),
            "no vars here"
        );
        // An unterminated reference is not a reference: left verbatim.
        assert_eq!(
            Env::expand_with("half ${OPEN", env, fails).unwrap(),
            "half ${OPEN"
        );
    }

    #[test]
    fn env_unset_without_default_errors() {
        let env = |_: &str| None;
        let err = Env::expand_with("${MISSING}", env, Unset::Fails).unwrap_err();
        assert_eq!(err.0, "MISSING");
    }

    /// A structural pass reads shapes, not secrets: an unset variable stands in
    /// rather than failing, and the placeholder is something the shape checks
    /// downstream (https, containment) accept.
    #[test]
    fn env_unset_stands_in_for_a_structural_pass() {
        let env = |_: &str| None;
        assert_eq!(
            Env::expand_with("${MISSING}", env, Unset::Stands).unwrap(),
            Unset::PLACEHOLDER
        );
        // ...and a default the author wrote still outranks the stand-in.
        assert_eq!(
            Env::expand_with("${MISSING:-fallback}", env, Unset::Stands).unwrap(),
            "fallback"
        );
    }

    /// The guard is scoped: a read after it drops fails again, so nothing but
    /// the pass that asked for it ever sees a placeholder.
    #[test]
    fn the_structural_guard_restores_the_previous_mode() {
        assert_eq!(Structural::mode(), Unset::Fails);
        {
            let _shape = Structural::begin();
            assert_eq!(Structural::mode(), Unset::Stands);
            // Nesting is the shape a profile check inside a theme parse takes.
            let _inner = Structural::begin();
            assert_eq!(Structural::mode(), Unset::Stands);
        }
        assert_eq!(Structural::mode(), Unset::Fails);
    }
}
