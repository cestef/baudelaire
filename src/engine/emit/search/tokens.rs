//! The one tokenizer: how a word becomes an index key, here and in the client.

/// Index keys, as both the build and the client's `tokenize` derive them.
pub(super) struct Tokens;

impl Tokens {
    /// The characters an index key keeps, as the body of a JavaScript character
    /// class, which the generated tokenizer builds its regex from so the two
    /// cannot drift apart.
    ///
    /// It spells the set [`char::is_alphanumeric`] accepts, which is wider than
    /// `\p{L}`: that alone drops the Indic, Arabic and Hebrew marks the index
    /// keeps.
    pub(super) const KEPT: &'static str = r"\p{Alphabetic}\p{N}";

    /// Every index key in `text`: split on whitespace, lowercased, stripped to
    /// alphanumerics, empties dropped.
    pub(super) fn of(text: &str) -> impl Iterator<Item = String> + '_ {
        text.split_whitespace()
            .map(Self::normalize)
            .filter(|token| !token.is_empty())
    }

    /// One word reduced to its index key.
    ///
    /// The client's `tokenize` must derive the same key, and strips by
    /// [`KEPT`](Self::KEPT) to do it. What it cannot take from here is the
    /// order: lowercase *before* stripping, since a codepoint like `İ`
    /// lowercases to a letter plus a combining mark that is not alphanumeric.
    pub(super) fn normalize(word: &str) -> String {
        word.chars()
            .flat_map(char::to_lowercase)
            .filter(|c| c.is_alphanumeric())
            .collect()
    }

    /// Whether a token is one the index keeps: long enough, and not a word the
    /// site excludes.
    pub(super) fn kept(token: &str, minimum: usize, stopwords: &[String]) -> bool {
        token.chars().count() >= minimum && !stopwords.iter().any(|word| word == token)
    }
}
