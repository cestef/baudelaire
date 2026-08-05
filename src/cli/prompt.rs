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
use crate::remote::Interaction;

/// One selectable option: the words that choose it (the first is shown as the
/// label, all are accepted as input), the value it yields when picked, and an
/// optional line describing it, shown while it is highlighted.
struct Opt<'a, T> {
    keys: Vec<&'a str>,
    value: T,
    about: Option<&'a str>,
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
        Self {
            question,
            options: Vec::new(),
            default: 0,
        }
    }

    /// Add an option. `keys[0]` is its label; every key matches typed input.
    pub fn option(mut self, keys: &[&'a str], value: T) -> Self {
        self.options.push(Opt {
            keys: keys.to_vec(),
            value,
            about: None,
        });
        self
    }

    /// Add an option chosen by one word, which is also its label. What a prompt
    /// built from a table wants: each row is its own key, and a borrowed slice
    /// per row would need somewhere to live for as long as the prompt does.
    pub fn one(self, key: &'a str, value: T) -> Self {
        self.option(std::slice::from_ref(&key), value)
    }

    /// Mark the most recently added option as the default (taken on empty input).
    pub fn default(mut self) -> Self {
        self.default = self.options.len().saturating_sub(1);
        self
    }

    /// Describe the most recently added option, in the [`Prompt::default`]
    /// style. The line shows under the chips while that option is highlighted,
    /// which is what makes a prompt over a table of names readable: the names
    /// alone (`albatros`, `spleen`) say nothing to someone choosing.
    pub fn about(mut self, line: &'a str) -> Self {
        if let Some(last) = self.options.last_mut() {
            last.about = Some(line);
        }
        self
    }

    /// Read a choice with the arrow keys: ←/→ (or ↑/↓) move, a letter jumps to a
    /// matching option, Enter confirms, Esc takes the default. Redraws in place.
    /// Without an interactive terminal (piped/CI) it returns the default at once.
    /// Renders on stderr, like every other CLI line: stdout stays data-only.
    pub fn ask(&self) -> Result<T> {
        let term = Term::stderr();
        if !term.is_term() {
            return Ok(self.chosen(self.default));
        }
        let last = self
            .options
            .len()
            .checked_sub(1)
            .expect("Prompt built with no options");
        let mut selected = self.default;
        // How many lines the last draw left behind, so the next one can take
        // them back: the chip row is one, and an option describing itself adds
        // another.
        let mut drawn = 0;
        loop {
            self.render(&term, selected, false, &mut drawn)?;
            // A read failure (EOF, closed terminal) falls back to the default,
            // as the module contract promises: it must never error out of init.
            let Ok(key) = term.read_key() else {
                self.render(&term, self.default, true, &mut drawn)?;
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
                    if let Some(i) = self
                        .options
                        .iter()
                        .position(|o| o.keys.iter().any(|k| k.starts_with(c)))
                    {
                        selected = i;
                    }
                }
                Key::Enter => {
                    self.render(&term, selected, true, &mut drawn)?;
                    return Ok(self.chosen(selected));
                }
                Key::Escape => {
                    self.render(&term, self.default, true, &mut drawn)?;
                    return Ok(self.chosen(self.default));
                }
                _ => {}
            }
        }
    }

    fn chosen(&self, i: usize) -> T {
        self.options[i].value.clone()
    }

    /// Redraw the prompt in place. While choosing, options render as a row of
    /// chips with the selection highlighted, and the highlighted one's
    /// description (if it has one) below them; once `done`, collapse to
    /// `✓ question › choice`.
    ///
    /// `drawn` carries how many lines the previous draw wrote, since that is
    /// what this one has to erase: a prompt whose options describe themselves
    /// is two lines tall, and a fixed `clear_line` would leave every hint it
    /// scrolled past on the screen.
    fn render(&self, term: &Term, selected: usize, done: bool, drawn: &mut usize) -> Result<()> {
        if *drawn > 0 {
            term.clear_last_lines(*drawn)?;
        }
        // The final line stays on screen, so it counts as nothing to erase.
        *drawn = 0;
        if done {
            let label = self.options[selected].keys[0];
            let line = format!(
                "{} {} {} {}",
                "✓".green().bold(),
                self.question.bold(),
                "›".dimmed(),
                label.cyan()
            );
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
        term.write_line(&format!(
            "{} {} {} {}  {hint}",
            "?".cyan().bold(),
            self.question.bold(),
            "›".dimmed(),
            chips
        ))?;
        *drawn += 1;
        if let Some(about) = self.options[selected].about {
            term.write_line(&format!("  {}", about.dimmed()))?;
            *drawn += 1;
        }
        Ok(())
    }
}

/// A styled hidden-input prompt for secrets: the same `? question` prefix as
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
        // through anstream so styling strips on a pipe and honors NO_COLOR like
        // every other CLI line; read hidden via `Term`.
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
        Self {
            question,
            default: "",
        }
    }

    /// The value returned (and shown as a hint) when the answer is left blank.
    pub fn default(mut self, value: &'a str) -> Self {
        self.default = value;
        self
    }

    /// Render the prompt (on stderr, keeping stdout data-only) and read one
    /// line, returning the trimmed answer or the default on an empty line or
    /// EOF.
    pub fn ask(&self) -> Result<String> {
        // through anstream so styling strips on a pipe and honors NO_COLOR like
        // every other CLI line.
        if self.default.is_empty() {
            anstream::eprint!("{} {} ", "?".cyan().bold(), self.question.bold());
        } else {
            let hint = format!("({})", self.default);
            anstream::eprint!(
                "{} {} {} ",
                "?".cyan().bold(),
                self.question.bold(),
                hint.dimmed()
            );
        }
        anstream::stderr().flush()?;
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line)? == 0 {
            eprintln!();
        }
        let answer = line.trim();
        Ok(if answer.is_empty() {
            self.default.to_owned()
        } else {
            answer.to_owned()
        })
    }
}

/// Terminal-backed [`Interaction`]: yes/no confirmation through [`Prompt`] and
/// hidden secret entry through [`Secret`]. One impl, shared by every command
/// that prompts, rather than a copy per command.
pub struct Tty;

impl Interaction for Tty {
    /// Whether a question would reach anyone. The same test [`Prompt::ask`] and
    /// [`Secret::ask`] make before falling back to their defaults, so a caller
    /// that refuses to accept a default refuses in exactly the cases they would
    /// have invented one.
    fn interactive(&self) -> bool {
        Term::stderr().is_term()
    }

    fn confirm(&self, prompt: &str) -> Result<bool> {
        Prompt::new(prompt)
            .option(&["yes"], true)
            .option(&["no"], false)
            .default()
            .ask()
    }

    fn secret(&self, label: &str) -> Result<Option<String>> {
        Secret::new(label).ask()
    }
}
