//! The value parser every `config` verb takes its key through.
//!
//! It validates against the dispatch tables *and* answers with them, so a
//! generated completion script offers every key the config accepts and a typo
//! is refused with the nearest one.

use clap::builder::{PossibleValue, TypedValueParser};
use clap::{Arg, Command, Error};

use crate::config::key::Key;
use crate::config::reference::Reference;
use crate::error::cli::UnknownKey;

/// A dotted config key, as typed.
#[derive(Clone, Copy)]
pub struct Parser;

impl TypedValueParser for Parser {
    type Value = String;

    fn parse_ref(
        &self,
        cmd: &Command,
        _arg: Option<&Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, Error> {
        let key = value.to_string_lossy().into_owned();
        if Key::new(&key).resolved().is_none() {
            let refused = UnknownKey::at(&key);
            return Err(Error::raw(
                clap::error::ErrorKind::InvalidValue,
                format!("{refused}\n  {}\n", refused.help),
            )
            .with_cmd(cmd));
        }
        Ok(key)
    }

    /// What a completion script offers: every key the tables declare. A key
    /// under a name the author chose is accepted too, and cannot be listed
    /// here, since only their config knows what they called it.
    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
        Some(Box::new(
            Reference::new()
                .entries()
                .iter()
                .map(|entry| PossibleValue::new(entry.path.clone()).help(entry.doc))
                .collect::<Vec<_>>()
                .into_iter(),
        ))
    }
}
