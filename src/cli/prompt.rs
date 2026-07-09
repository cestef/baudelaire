//! Interactive terminal prompts: a small, styled, reusable "pick one" widget.
//!
//! [`Prompt`] is generic over the value each option yields, so a caller gets a
//! typed answer back rather than a raw string. Options are added fluently, one is
//! marked the default, and [`Prompt::ask`] renders a colored line, accepts any of
//! an option's aliases, re-asks on unrecognized input, and falls back to the
//! default on an empty line or closed input.

use std::io::Write;

use owo_colors::OwoColorize;

use crate::error::Result;

/// One selectable option: the words that choose it (the first is shown as the
/// label, all are accepted as input) and the value it yields when picked.
struct Opt<'a, T> {
    keys: &'a [&'a str],
    value: T,
}

/// A styled single-choice prompt. Build it with [`Prompt::new`], add options with
/// [`Prompt::option`] / [`Prompt::default`], then [`Prompt::ask`].
pub struct Prompt<'a, T> {
    question: &'a str,
    options: Vec<Opt<'a, T>>,
    default: usize,
}

impl<'a, T: Clone> Prompt<'a, T> {
    pub fn new(question: &'a str) -> Self {
        Self { question, options: Vec::new(), default: 0 }
    }

    /// Add an option. `keys[0]` is its label; every key matches typed input.
    pub fn option(mut self, keys: &'a [&'a str], value: T) -> Self {
        self.options.push(Opt { keys, value });
        self
    }

    /// Mark the most recently added option as the default (taken on empty input).
    pub fn default(mut self) -> Self {
        self.default = self.options.len().saturating_sub(1);
        self
    }

    /// Render the prompt and read a choice. Re-asks on unrecognized input;
    /// returns the default on an empty line or EOF (closed/piped input).
    pub fn ask(&self) -> Result<T> {
        loop {
            self.render()?;
            let mut line = String::new();
            if std::io::stdin().read_line(&mut line)? == 0 {
                println!();
                return Ok(self.chosen(self.default));
            }
            let input = line.trim().to_ascii_lowercase();
            if input.is_empty() {
                return Ok(self.chosen(self.default));
            }
            match self.options.iter().position(|o| o.keys.contains(&input.as_str())) {
                Some(i) => return Ok(self.chosen(i)),
                None => {
                    let labels = self.options.iter().map(|o| o.keys[0]).collect::<Vec<_>>().join(", ");
                    eprintln!("  {} please answer with one of: {}", "!".yellow(), labels.dimmed());
                }
            }
        }
    }

    fn chosen(&self, i: usize) -> T {
        self.options[i].value.clone()
    }

    /// A line like `? question › git / jj / no`, the default emphasized.
    fn render(&self) -> Result<()> {
        let sep = " / ".dimmed().to_string();
        let options = self
            .options
            .iter()
            .enumerate()
            .map(|(i, o)| {
                if i == self.default {
                    o.keys[0].bold().underline().to_string()
                } else {
                    o.keys[0].dimmed().to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(&sep);
        print!("{} {} {} {} ", "?".cyan().bold(), self.question.bold(), "›".dimmed(), options);
        std::io::stdout().flush()?;
        Ok(())
    }
}
