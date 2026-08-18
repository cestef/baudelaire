//! `baudelaire config explain`: what one key is, what it holds, and which layer
//! it came from.

use std::fmt;
use std::path::Path;

use clap::Args;
use owo_colors::OwoColorize;

use super::super::{Cx, Run, help};
use super::key;
use crate::config::explain::{Sighting, Sightings};
use crate::config::values::source::{Layer, Resolved};
use crate::config::values::{Tree, Value};
use crate::error::Result;
use crate::ui::{ARROW_LABEL, Highlighted, markup};

#[derive(Args, Debug, Clone)]
#[command(after_help = ExplainArgs::examples())]
pub struct ExplainArgs {
    /// The dotted key path, e.g. `content.collections.permalink`.
    #[arg(value_parser = key::Parser, hide_possible_values = true)]
    pub key: String,
}

impl ExplainArgs {
    /// Appended to `config explain --help`.
    fn examples() -> String {
        help::Table::examples(&[
            ("baudelaire config explain paths.dist", "One key"),
            (
                "baudelaire config explain content.collections.permalink",
                "A key under a name you chose",
            ),
        ])
        .to_string()
    }
}

impl Run for ExplainArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        use crate::config::key::Key;
        use crate::config::reference::Reference;
        use crate::error::cli::UnknownKey;

        let table = Key::new(&self.key)
            .resolved()
            .ok_or_else(|| UnknownKey::at(&self.key))?;
        let reference = Reference::at(&table).ok_or_else(|| UnknownKey::at(&self.key))?;
        let entry = &reference.entries()[0];
        let title = markup!("`{}`  {}", &self.key, entry.kind.label());
        cx.ui.section(cx.ui.markup(&title));
        cx.ui.detail(cx.ui.markup(entry.doc));

        let sources = cx.cli.sources()?;
        let sightings = Sightings::of(sources.config(), &self.key)?;
        let Some(resolved) = sources.resolve(&self.key) else {
            cx.ui.item("not held by this config at all");
            return Ok(());
        };
        cx.ui.blank();
        let trail = Trail::new(&resolved, &sightings, &cx.cli.global.config);
        cx.ui.arrow(
            &trail.label(VALUE),
            Held::at(resolved.value, trail.column()),
        );
        cx.ui.tree(&trail.rows());
        Ok(())
    }
}

/// The arrow's own label, which the layers beneath it line up with.
const VALUE: &str = "value";

/// A value written where the line it opens on left off: a block continues under
/// itself rather than under the marker.
struct Held<'a> {
    value: &'a Value,
    column: usize,
}

impl<'a> Held<'a> {
    fn at(value: &'a Value, column: usize) -> Self {
        Self { value, column }
    }
}

impl fmt::Display for Held<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let written = Tree::of(self.value).to_string();
        let written = written.trim_end();
        if written.is_empty() {
            return write!(f, "{}", "unset".dimmed());
        }
        let indent = " ".repeat(self.column);
        let highlighted = Highlighted::new(written, "kdl").to_string();
        let mut lines = highlighted.lines();
        let Some(first) = lines.next() else {
            return Ok(());
        };
        f.write_str(first)?;
        for line in lines {
            write!(f, "\n{indent}{line}")?;
        }
        Ok(())
    }
}

/// Where a value came from, layer by layer: the location each layer wrote it
/// at, and what it held there.
struct Trail<'a> {
    resolved: &'a Resolved<'a>,
    sightings: &'a Sightings,
    config: &'a Path,
}

impl<'a> Trail<'a> {
    fn new(resolved: &'a Resolved<'a>, sightings: &'a Sightings, config: &'a Path) -> Self {
        Self {
            resolved,
            sightings,
            config,
        }
    }

    /// Where one layer wrote the value: the file and line for a layer that is
    /// text, the layer's own name otherwise. The file is named by its last
    /// component, the run having read one config file at all.
    fn at(&self, layer: &Layer) -> String {
        let named = self.config.file_name().map_or_else(
            || self.config.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        match (layer, self.written(layer)) {
            (Layer::File, Some(sighting)) => format!("{named}:{}", sighting.line),
            (Layer::Profile(name), Some(sighting)) => format!("profile {name}:{}", sighting.line),
            (layer, _) => layer.to_string(),
        }
    }

    /// The sighting a layer's value was written at, for the layers that are
    /// text.
    fn written(&self, layer: &Layer) -> Option<&Sighting> {
        self.sightings.all().iter().rfind(|sighting| match layer {
            Layer::File => sighting.profile.is_none(),
            Layer::Profile(name) => sighting.profile.as_deref() == Some(name.as_str()),
            Layer::Default | Layer::Theme(_) => false,
        })
    }

    /// Every layer to name, oldest first: the ones that changed the value, plus
    /// the ones that write the key without changing it, which a reader looking
    /// for their own line still has to find.
    fn layers(&self) -> Vec<(Layer, &Value)> {
        let mut layers: Vec<(Layer, &Value)> = self
            .resolved
            .trail
            .iter()
            .map(|(layer, value)| ((*layer).clone(), *value))
            .collect();
        for sighting in self.sightings.all() {
            let layer = sighting
                .profile
                .as_ref()
                .map_or(Layer::File, |profile| Layer::Profile(profile.clone()));
            if !layers.iter().any(|(held, _)| *held == layer) {
                layers.push((layer, self.resolved.value));
            }
        }
        layers.sort_by_key(|(layer, _)| layer.rank());
        layers
    }

    /// The width the arrow's label is padded to: wide enough for every layer's
    /// own label, which sits one column further left under its tree marker.
    fn width(&self) -> usize {
        let longest = self
            .layers()
            .iter()
            .map(|(layer, _)| self.at(layer).chars().count())
            .max()
            .unwrap_or_default();
        ARROW_LABEL.max(longest + 1)
    }

    /// The column the arrow's value and every layer's line up at.
    fn column(&self) -> usize {
        self.width() + 5
    }

    /// A label padded to the arrow's own width.
    fn label(&self, text: &str) -> String {
        format!("{text:<width$}", width = self.width())
    }

    /// One row per layer that had something to say, oldest first. A block is
    /// named by where it was written and not printed again under every layer.
    fn rows(&self) -> Vec<String> {
        let block = matches!(self.resolved.value, Value::Node { keys, .. } if !keys.is_empty());
        self.layers()
            .iter()
            .map(|(layer, value)| {
                let at = self.at(layer);
                if block {
                    return at.dimmed().to_string();
                }
                let padded = format!("{at:<width$}", width = self.width() - 1);
                format!("{} {}", padded.dimmed(), Held::at(value, self.column()))
            })
            .collect()
    }
}
