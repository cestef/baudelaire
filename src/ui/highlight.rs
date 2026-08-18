//! Syntax highlighting for the terminal, over the same vocabulary the built
//! pages are marked up with: one grammar, one token table, two palettes.

use std::fmt;

use owo_colors::{OwoColorize, Stream::Stdout, Style};
use typst::ecow::EcoString;

use crate::config::Token;
use crate::world::rules::Grammar;

/// A text highlighted as `lang`, written as ANSI when the stream takes colour
/// and as the text itself when it does not.
pub struct Highlighted<'a> {
    text: &'a str,
    lang: &'a str,
}

impl<'a> Highlighted<'a> {
    pub fn new(text: &'a str, lang: &'a str) -> Self {
        Self { text, lang }
    }

    /// The terminal palette, the counterpart of the CSS classes
    /// [`HighlightConfig::class`](crate::config::HighlightConfig::class) names.
    fn style(token: Token) -> Style {
        let style = Style::new();
        match token {
            Token::Comment => style.dimmed(),
            Token::String | Token::Escape | Token::Raw => style.green(),
            Token::Number | Token::Constant => style.magenta(),
            Token::Keyword | Token::Operator | Token::Strong => style.blue().bold(),
            Token::Function | Token::Tag | Token::Heading => style.cyan().bold(),
            Token::Type | Token::Namespace | Token::Attribute | Token::Property => style.yellow(),
            Token::Variable | Token::Parameter | Token::Label => style.cyan(),
            Token::Link => style.blue().underline(),
            Token::Emph => style.italic(),
            Token::Invalid => style.red().bold(),
            Token::Punctuation => style,
        }
    }
}

impl fmt::Display for Highlighted<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lines: Vec<EcoString> = self.text.lines().map(EcoString::from).collect();
        let mut out = vec![String::new(); lines.len()];
        Grammar::named(self.lang).tokens(&lines, &mut |piece| {
            let styled = match piece.token {
                Some((token, _)) => piece
                    .text
                    .if_supports_color(Stdout, |text| text.style(Self::style(token)))
                    .to_string(),
                None => piece.text.to_owned(),
            };
            out[piece.line].push_str(&styled);
        });
        f.write_str(&out.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::Highlighted;

    /// Colour is decided by the stream, and a test's is not a terminal, so what
    /// is asserted here is that nothing is lost or reordered.
    #[test]
    fn every_character_survives_the_pass() {
        for (lang, text) in [
            ("kdl", "site \"T\"\npaths {\n  dist \"public\"\n}"),
            ("json", "{\"a\": [1, 2]}"),
            ("nothing-parses-this", "a b c"),
        ] {
            assert_eq!(Highlighted::new(text, lang).to_string(), text, "{lang}");
        }
    }
}
