//! The system shell: what it is called, how it takes a command line, and how a
//! path is spelled as one word for it.

use std::path::Path;
use std::process::Command;

/// The shell a command line is handed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    /// POSIX `sh`, taking its command after `-c`.
    Sh,
    /// Windows `cmd`, taking its command after `/C`.
    Cmd,
}

impl Shell {
    /// The shell of the platform this build is running on.
    pub const HOST: Self = if cfg!(windows) { Self::Cmd } else { Self::Sh };

    const fn program(self) -> &'static str {
        match self {
            Self::Sh => "sh",
            Self::Cmd => "cmd",
        }
    }

    /// The flag that makes it read its command from the next argument.
    const fn flag(self) -> &'static str {
        match self {
            Self::Sh => "-c",
            Self::Cmd => "/C",
        }
    }

    /// The quote this shell wraps a word in, and what a literal one of those
    /// becomes inside it.
    const fn quoting(self) -> (char, &'static str) {
        match self {
            Self::Sh => ('\'', r"'\''"),
            Self::Cmd => ('"', "\"\""),
        }
    }

    /// `line`, ready to spawn in `cwd`, with stdio still the caller's to choose.
    pub fn command(self, line: &str, cwd: &Path) -> Command {
        let mut command = Command::new(self.program());
        command.arg(self.flag()).arg(line).current_dir(cwd);
        command
    }

    /// `path` as one word of a command line, whatever characters it holds.
    pub fn word(self, path: &Path) -> String {
        let (quote, escaped) = self.quoting();
        let mut word = String::from(quote);
        word.push_str(&path.to_string_lossy().replace(quote, escaped));
        word.push(quote);
        word
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::Shell;

    #[test]
    fn a_path_is_one_word_however_it_is_spelled() {
        assert_eq!(Shell::Sh.word(Path::new("/a dir/x.sh")), "'/a dir/x.sh'");
        assert_eq!(
            Shell::Cmd.word(Path::new(r"C:\a dir\x")),
            "\"C:\\a dir\\x\""
        );
    }

    #[test]
    fn a_quote_inside_a_path_cannot_close_the_word() {
        assert_eq!(Shell::Sh.word(Path::new("/it's")), r"'/it'\''s'");
        assert_eq!(Shell::Cmd.word(Path::new("/a\"b")), "\"/a\"\"b\"");
    }
}
