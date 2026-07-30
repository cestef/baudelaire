//! The UI strings baudelaire itself writes into generated pages.
//!
//! Generated listings (taxonomy indexes, paginated indexes) and redirect stubs
//! are markup baudelaire authors, not the site's templates, so their wording
//! cannot come from a layout. Each string here has an English default and is
//! overridable per language:
//!
//! ```kdl
//! languages {
//!   fr { strings { previous "← Précédent"; next "Suivant →" } }
//! }
//! ```

use crate::config::Config;

/// One language's view of the generated-page vocabulary.
pub struct Strings<'a> {
    config: &'a Config,
    lang: &'a str,
}

impl<'a> Strings<'a> {
    /// Every key baudelaire looks up, with its English default. The single
    /// source of truth: the lookup and the documentation both read this.
    const DEFAULTS: &'static [(&'static str, &'static str)] = &[
        ("previous", "← Previous"),
        ("next", "Next →"),
        ("page", "page"),
        ("redirecting", "Redirecting.."),
        // How a date is written out for a reader. `{month}` takes its word from
        // the `months` list, which is a list rather than a string and so is not
        // in this table; the rest are numbers off the date itself.
        ("date", "{month} {day}, {year}"),
    ];

    pub fn new(config: &'a Config, lang: &'a str) -> Self {
        Self { config, lang }
    }

    /// The string for `key` in this language: the language's own override, else
    /// the default language's, else the built-in English.
    pub fn get(&self, key: &str) -> &str {
        self.declared(self.lang, key)
            .or_else(|| self.declared(&self.config.lang, key))
            .or_else(|| Self::default(key))
            .unwrap_or_default()
    }

    /// A list a language declares, when it is an array of plain strings. The
    /// same language-then-default-then-nothing fallback [`get`](Self::get)
    /// uses, for the one key whose value is a list: `months`.
    pub fn list(&self, key: &str) -> Option<Vec<String>> {
        self.array(self.lang, key)
            .or_else(|| self.array(&self.config.lang, key))
    }

    /// An array of plain strings a language declares under `key`.
    fn array(&self, lang: &str, key: &str) -> Option<Vec<String>> {
        let crate::codegen::Value::Array(items) = self
            .config
            .strings(lang)
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)?
        else {
            return None;
        };
        items
            .iter()
            .map(|item| match item {
                crate::codegen::Value::Str(text) => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    /// A string a language declares, when it is a plain string value.
    fn declared(&self, lang: &str, key: &str) -> Option<&str> {
        self.config
            .strings(lang)
            .iter()
            .find(|(name, _)| name == key)
            .and_then(|(_, value)| match value {
                crate::codegen::Value::Str(text) => Some(text.as_str()),
                _ => None,
            })
    }

    fn default(key: &str) -> Option<&'static str> {
        Self::DEFAULTS
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, text)| *text)
    }
}

#[cfg(test)]
mod tests {
    use super::Strings;
    use crate::config::Config;

    fn config(kdl: &str) -> Config {
        Config::parse(kdl).expect("config")
    }

    #[test]
    fn a_language_overrides_the_built_in_default() {
        let cfg = config("lang \"en\"\nlanguages {\n  fr { strings { next \"Suivant →\" } }\n}");
        assert_eq!(Strings::new(&cfg, "fr").get("next"), "Suivant →");
        // Keys it does not declare keep the default.
        assert_eq!(Strings::new(&cfg, "fr").get("previous"), "← Previous");
        assert_eq!(Strings::new(&cfg, "en").get("next"), "Next →");
    }

    /// A site that retitles a string for its own default language gets it in
    /// every language that has not overridden it.
    #[test]
    fn the_default_language_supplies_the_fallback() {
        let cfg =
            config("lang \"en\"\nlanguages {\n  en { strings { next \"Onwards\" } }\n  fr { }\n}");
        assert_eq!(Strings::new(&cfg, "fr").get("next"), "Onwards");
    }

    #[test]
    fn an_unknown_key_is_empty() {
        let cfg = config("lang \"en\"");
        assert_eq!(Strings::new(&cfg, "en").get("nope"), "");
    }
}
