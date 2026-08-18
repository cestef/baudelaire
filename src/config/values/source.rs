//! The layers a config is built out of, and which one a value came from.
//!
//! ```text
//! default   the built-in `Default` impls
//! theme     the theme's `theme.kdl`, the floor a site stands on
//! file      the site's own `config.kdl`
//! profile   the profile applied over it
//! ```
//!
//! A layer is one variant plus one line in [`Sources::of`]: nothing else in the
//! crate has to learn about it.

use std::fmt;
use std::path::Path;

use super::Value;
use crate::config::Config;
use crate::error::Result;

/// One layer of a config, in the order layers apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Layer {
    /// What a site holds before anything is written down.
    Default,
    /// A theme's `theme.kdl`, named as the config names it.
    Theme(String),
    /// The site's own config file.
    File,
    /// A profile applied over it.
    Profile(String),
}

impl Layer {
    /// Where this layer sits in the order they apply.
    pub fn rank(&self) -> usize {
        match self {
            Self::Default => 0,
            Self::Theme(_) => 1,
            Self::File => 2,
            Self::Profile(_) => 3,
        }
    }
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => f.write_str("default"),
            Self::Theme(theme) => write!(f, "theme {theme}"),
            Self::File => f.write_str("config"),
            Self::Profile(profile) => write!(f, "profile {profile}"),
        }
    }
}

/// One layer and everything it holds.
pub struct Source {
    pub layer: Layer,
    values: Value,
}

impl Source {
    /// What this layer holds at `key`, or `None` for a key it does not reach.
    pub fn at(&self, key: &str) -> Option<&Value> {
        self.values.at(key)
    }
}

/// Every layer of one config, outermost last, and the config they resolve to.
pub struct Sources {
    layers: Vec<Source>,
    config: Config,
}

impl Sources {
    /// Rebuild each layer of the config `text` describes, from the same inputs
    /// [`Config::load`] reads it with.
    ///
    /// Each layer is the whole config *as of* that layer, so a key's origin is
    /// the last one that changed it, and a key nothing writes stays where it
    /// started.
    pub fn of(text: &str, root: &Path, theme: Option<&str>, profile: Option<&str>) -> Result<Self> {
        let mut layers = vec![Source {
            layer: Layer::Default,
            values: Config::default().values(),
        }];
        let mut config = Config::load(text, root, theme)?;
        if let Some(named) = &config.theme
            && let Some(floor) = config.beneath()?
        {
            layers.push(Source {
                layer: Layer::Theme(named.clone()),
                values: floor.values(),
            });
        }
        layers.push(Source {
            layer: Layer::File,
            values: config.values(),
        });
        if let Some(name) = profile {
            config = config.with_profile(name)?;
            layers.push(Source {
                layer: Layer::Profile(name.to_owned()),
                values: config.values(),
            });
        }
        Ok(Self { layers, config })
    }

    /// The config these layers resolve to, profile and all.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Everything the outermost layer holds.
    pub fn values(&self) -> &Value {
        &self
            .layers
            .last()
            .expect("the default layer is always there")
            .values
    }

    /// What `key` is set to, and the layer that last had something to say about
    /// it, or `None` for a key no layer holds.
    pub fn resolve(&self, key: &str) -> Option<Resolved<'_>> {
        let mut resolved: Option<Resolved<'_>> = None;
        for source in &self.layers {
            let Some(value) = source.at(key) else {
                continue;
            };
            match &mut resolved {
                Some(held) if held.value == value => {}
                Some(held) => {
                    held.value = value;
                    held.trail.push((&source.layer, value));
                }
                None => {
                    resolved = Some(Resolved {
                        value,
                        trail: vec![(&source.layer, value)],
                    });
                }
            }
        }
        resolved
    }
}

/// One key's effective value, and every layer that had something different to
/// say on the way to it.
pub struct Resolved<'a> {
    pub value: &'a Value,
    /// Oldest first, one entry per layer that changed the value; the last of
    /// them is the layer the value came from.
    pub trail: Vec<(&'a Layer, &'a Value)>,
}

impl<'a> Resolved<'a> {
    /// The layer the value came from.
    pub fn layer(&self) -> &'a Layer {
        self.trail
            .last()
            .map(|(layer, _)| *layer)
            .expect("a resolved value came from a layer")
    }
}
