//! The one tokenizer: how a word becomes an index key, here and in the client.

/// Index keys, as both the build and the client's `tokenize` derive them.
pub(super) struct Tokens;

impl Tokens {
    /// Every index key in `text`: split on whitespace, lowercased, stripped to
    /// alphanumerics, empties dropped.
    pub(super) fn of(text: &str) -> impl Iterator<Item = String> + '_ {
        text.split_whitespace()
            .map(Self::normalize)
            .filter(|token| !token.is_empty())
    }

    /// One word reduced to its index key.
    ///
    /// The client's `tokenize` must derive the same key: lowercase *before*
    /// stripping, since a codepoint like `İ` lowercases to a letter plus a
    /// combining mark that is not alphanumeric, and match `char::is_alphanumeric`
    /// as `\p{Alphabetic}\p{N}` rather than the narrower `\p{L}`, which drops
    /// marks the index keeps.
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
