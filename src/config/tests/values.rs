//! Values a key refuses for reasons of its own: not the wrong KDL type, and not
//! a name outside a fixed set, but a string that is the wrong *thing*.

use super::{code, err, parse};

/// Nothing runs `editor` through a shell, so a whole command line in one string
/// names a program that does not exist.
#[test]
fn err_a_command_line_written_as_one_word_is_refused() {
    let config = "serve {\n  editor \"code --goto {file}\"\n}";
    assert_eq!(code(config), "baudelaire::config::command_line");
    let rendered = err(config);
    assert!(
        rendered.contains("is a command line, not a program"),
        "{rendered}"
    );
    assert!(rendered.contains("--goto"), "{rendered}");
}

#[test]
fn a_command_written_as_words_parses() {
    let cfg = parse("serve {\n  editor \"code\" \"--goto\" \"{file}:{line}\"\n}");
    assert_eq!(cfg.serve.editor.len(), 3);
    assert_eq!(parse("serve {\n  editor \"vi\"\n}").serve.editor, ["vi"]);
}

/// `footnotes` names elements, so a name no element can carry is refused here
/// rather than failing silently at render.
#[test]
fn err_a_name_no_element_answers_to_is_refused() {
    let config = "html {\n  footnotes \"not a tag\"\n}";
    assert_eq!(code(config), "baudelaire::config::not_an_element");
    let rendered = err(config);
    assert!(rendered.contains("is not an HTML element"), "{rendered}");
    assert!(rendered.contains("tag name"), "{rendered}");
}

#[test]
fn an_element_name_the_dom_accepts_parses() {
    let cfg = parse("html {\n  footnotes \"article\" \"my-aside\"\n}");
    assert_eq!(cfg.html.footnotes.targets().len(), 2);
}
