//! Interactive terminal prompts: small, styled, reusable widgets.
//!
//! [`Prompt`] is a "pick one" selector, generic over the value each option
//! yields, so a caller gets a typed answer rather than a raw string; [`Input`] is
//! a free-text question with an optional default. Both render the same
//! `? question` prefix, and both fall back to their default on an empty line or
//! closed input (EOF).

use std::io::Write;

use console::{Key, Term};
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

    /// Read a choice with the arrow keys: ←/→ (or ↑/↓) move, a letter jumps to a
    /// matching option, Enter confirms, Esc takes the default. Redraws in place.
    /// Without an interactive terminal (piped/CI) it returns the default at once.
    pub fn ask(&self) -> Result<T> {
        let term = Term::stdout();
        if !term.is_term() {
            return Ok(self.chosen(self.default));
        }
        let last = self.options.len().checked_sub(1).expect("Prompt built with no options");
        let mut selected = self.default;
        loop {
            self.render(&term, selected, false)?;
            // A read failure (EOF, closed terminal) falls back to the default,
            // as the module contract promises — it must never error out of init.
            let Ok(key) = term.read_key() else {
                self.render(&term, self.default, true)?;
                return Ok(self.chosen(self.default));
            };
            match key {
                Key::ArrowLeft | Key::ArrowUp | Key::BackTab => {
                    selected = if selected == 0 { last } else { selected - 1 };
                }
                Key::ArrowRight | Key::ArrowDown | Key::Tab => {
                    selected = if selected == last { 0 } else { selected + 1 };
                }
                Key::Char(c) => {
                    let c = c.to_ascii_lowercase();
                    if let Some(i) = self.options.iter().position(|o| o.keys.iter().any(|k| k.starts_with(c))) {
                        selected = i;
                    }
                }
                Key::Enter => {
                    self.render(&term, selected, true)?;
                    return Ok(self.chosen(selected));
                }
                Key::Escape => {
                    self.render(&term, self.default, true)?;
                    return Ok(self.chosen(self.default));
                }
                _ => {}
            }
        }
    }

    fn chosen(&self, i: usize) -> T {
        self.options[i].value.clone()
    }

    /// Redraw the prompt line in place. While choosing, options render as a row of
    /// chips with the selection highlighted; once `done`, collapse to `✓ question ›
    /// choice`.
    fn render(&self, term: &Term, selected: usize, done: bool) -> Result<()> {
        term.clear_line()?;
        if done {
            let label = self.options[selected].keys[0];
            let line =
                format!("{} {} {} {}", "✓".green().bold(), self.question.bold(), "›".dimmed(), label.cyan());
            term.write_line(&line)?;
            return Ok(());
        }
        let chips = self
            .options
            .iter()
            .enumerate()
            .map(|(i, o)| {
                let chip = format!(" {} ", o.keys[0]);
                if i == selected {
                    chip.black().on_cyan().to_string()
                } else {
                    chip.dimmed().to_string()
                }
            })
            .collect::<String>();
        let hint = "(←/→, enter)".dimmed();
        term.write_str(&format!(
            "{} {} {} {}  {hint}",
            "?".cyan().bold(),
            self.question.bold(),
            "›".dimmed(),
            chips
        ))?;
        Ok(())
    }
}

/// A styled hidden-input prompt for secrets — the same `? question` prefix as
/// [`Input`], but the typed characters never echo. Returns `None` on a
/// non-terminal (nothing to read) or an empty answer, so a caller can fall back.
pub struct Secret<'a> {
    question: &'a str,
}

impl<'a> Secret<'a> {
    pub fn new(question: &'a str) -> Self {
        Self { question }
    }

    /// Render the prompt and read one hidden line, or `None` when there is no
    /// terminal to read from or the answer is blank.
    pub fn ask(&self) -> Result<Option<String>> {
        let term = Term::stderr();
        if !term.is_term() {
            return Ok(None);
        }
        // Prompt through anstream so styling strips on a non-terminal and honors
        // NO_COLOR, matching every other CLI line; read hidden via `Term`.
        anstream::eprint!("{} {} ", "?".cyan().bold(), self.question.bold());
        anstream::stderr().flush()?;
        let secret = term.read_secure_line()?;
        let secret = secret.trim();
        Ok((!secret.is_empty()).then(|| secret.to_owned()))
    }
}

/// A styled free-text prompt with an optional default, shown in parentheses and
/// returned on an empty answer.
pub struct Input<'a> {
    question: &'a str,
    default: &'a str,
}

impl<'a> Input<'a> {
    pub fn new(question: &'a str) -> Self {
        Self { question, default: "" }
    }

    /// The value returned (and shown as a hint) when the answer is left blank.
    pub fn default(mut self, value: &'a str) -> Self {
        self.default = value;
        self
    }

    /// Render the prompt and read one line, returning the trimmed answer or the
    /// default on an empty line or EOF.
    pub fn ask(&self) -> Result<String> {
        // Through anstream so styling strips on a non-terminal and honors
        // NO_COLOR, like every other CLI line.
        if self.default.is_empty() {
            anstream::print!("{} {} ", "?".cyan().bold(), self.question.bold());
        } else {
            let hint = format!("({})", self.default);
            anstream::print!("{} {} {} ", "?".cyan().bold(), self.question.bold(), hint.dimmed());
        }
        anstream::stdout().flush()?;
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line)? == 0 {
            println!();
        }
        let answer = line.trim();
        Ok(if answer.is_empty() { self.default.to_owned() } else { answer.to_owned() })
    }
}
