//! Values a key refuses for reasons of its own: not the wrong KDL type, and not
//! a name outside a fixed set, but a string that is the wrong *thing*.
//!
//! Both of these used to be reported as `unknown value`, whose message and help
//! promise a list of allowed names. Neither key has one -- a command is any
//! program on the machine, an element name is the whole of HTML -- so the
//! diagnostic described the mistake as something it was not.

use super::{code, err, parse};

/// Nothing runs `editor` through a shell, so a whole command line in one string
/// names a program that does not exist. Caught at the span the author wrote,
/// rather than as a spawn failure on the first alt-click.
#[test]
fn err_a_command_line_written_as_one_word_is_refused() {
    let config = "serve {\n  editor \"code --goto {file}\"\n}";
    assert_eq!(code(config), "baudelaire::config::command_line");
    let rendered = err(config);
    assert!(
        rendered.contains("is a command line, not a program"),
        "{rendered}"
    );
    // The help shows the spelling that works, placeholders and all.
    assert!(rendered.contains("--goto"), "{rendered}");
}

/// The program and its arguments as separate words is the shape that parses,
/// and a single word is still a program.
#[test]
fn a_command_written_as_words_parses() {
    let cfg = parse("serve {\n  editor \"code\" \"--goto\" \"{file}:{line}\"\n}");
    assert_eq!(cfg.serve.editor.len(), 3);
    assert_eq!(parse("serve {\n  editor \"vi\"\n}").serve.editor, ["vi"]);
}

/// `footnotes` names elements, so a name no element can carry is refused here,
/// where the span points at the word. Left to render it would fail silently, as
/// a container the page simply never has.
#[test]
fn err_a_name_no_element_answers_to_is_refused() {
    let config = "html {\n  footnotes \"not a tag\"\n}";
    assert_eq!(code(config), "baudelaire::config::not_an_element");
    let rendered = err(config);
    assert!(rendered.contains("is not an HTML element"), "{rendered}");
    // typst's own reason is carried through rather than restated.
    assert!(rendered.contains("tag name"), "{rendered}");
}

/// An element name the DOM does accept still parses, so the check is a filter
/// and not a prohibition.
#[test]
fn an_element_name_the_dom_accepts_parses() {
    let cfg = parse("html {\n  footnotes \"article\" \"my-aside\"\n}");
    assert_eq!(cfg.html.footnotes.targets().len(), 2);
}
