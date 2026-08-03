//! `manifest.webmanifest` generation.
//!
//! The [web app manifest][spec] a browser reads when a visitor installs the
//! site: what to call it, what to launch, what to draw before the first page
//! renders. One per language, beside that language's feeds and search index, so
//! installing from `/fr/` gives a French app that stays in the French site.
//!
//! Every member the build can derive it derives ([`Config::title`], the
//! language's root URL, an icon's media type), leaving the config to carry only
//! what an author knows.
//!
//! [spec]: https://www.w3.org/TR/appmanifest/

use serde::Serialize;

use super::{Emit, Processor, Site, Warn};
use crate::config::{Config, IconConfig, IconPurpose, ManifestConfig, Named};
use crate::error::warning::ManifestIcons;
use crate::error::{Artifact, Result};
use crate::mime::Mime;
use crate::render::Tail;

/// Emits one `manifest.webmanifest` per language when a `generate { manifest }`
/// block is configured.
pub(super) struct WebManifest;

impl Processor for WebManifest {
    fn enabled(&self, config: &Config) -> bool {
        config.generate.manifest.enabled
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        // A manifest with no icon parses, installs nothing, and looks like a
        // working feature: no browser offers to install a site it cannot draw
        // on a home screen. Warned once for the site rather than once per
        // language, since the icons are configured once.
        if site.config.generate.manifest.icons.is_empty() {
            out.warn(ManifestIcons);
        }
        for lang in site.config.langs() {
            let scope = site.config.scope(lang, "");
            let document = Document::new(site.config, lang);
            out.file(
                &site.dist(&[&scope, ManifestConfig::FILE]),
                &Artifact::WebManifest.json(&document)?,
            )?;
            out.note(format_args!("wrote {}/{}", scope, ManifestConfig::FILE));
        }
        Ok(())
    }
}

/// The manifest as it is serialized. Field names are the members the spec
/// defines, so what is written is read off this struct rather than assembled
/// from string keys.
#[derive(Serialize)]
struct Document<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    short_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
    /// The manifest's own language, and the direction it reads in: the app name
    /// is text like any other, and a launcher has no page to inherit either
    /// from.
    lang: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    dir: Option<&'a str>,
    start_url: String,
    scope: String,
    display: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    theme_color: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    background_color: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    icons: Vec<Icon>,
}

impl<'a> Document<'a> {
    fn new(config: &'a Config, lang: &'a str) -> Self {
        let cfg = &config.generate.manifest;
        Self {
            name: cfg.name.as_deref().unwrap_or_else(|| config.title(lang)),
            short_name: cfg.short.as_deref(),
            description: cfg.description.as_deref(),
            lang,
            dir: config.dir(lang),
            start_url: Self::url(config, cfg.start.as_deref(), lang),
            scope: Self::url(config, cfg.scope.as_deref(), lang),
            display: cfg.display.member(),
            theme_color: cfg.theme.as_deref(),
            background_color: cfg.background.as_deref(),
            icons: cfg
                .icons
                .iter()
                .map(|icon| Icon::new(config, icon))
                .collect(),
        }
    }

    /// A member naming a place in the site (`start_url`, `scope`), for one
    /// language: the authored path if there is one, else where that language's
    /// site begins.
    ///
    /// Localized either way, and for the same reason the defaults are: an app
    /// installed from `/fr/` launches into the French site and treats leaving it
    /// as leaving the app. A shared `start "/home/"` written outside a scope
    /// that *is* localized is a manifest a browser rejects, since `start_url`
    /// must sit within `scope`. Both then carry the base path, which a browser
    /// resolves neither of them against.
    fn url(config: &Config, authored: Option<&str>, lang: &str) -> String {
        config.prefixed(&config.localize(lang, authored.unwrap_or("/")))
    }
}

/// One entry of the `icons` array.
#[derive(Serialize)]
struct Icon {
    src: String,
    sizes: String,
    /// The media type, from the file's extension: it lets a launcher pick
    /// without fetching every candidate first.
    r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    purpose: Option<&'static str>,
}

impl Icon {
    fn new(config: &Config, icon: &IconConfig) -> Self {
        Self {
            src: config.prefixed(&icon.src),
            // `any` is what the spec calls an image that scales, which is the
            // honest answer for a vector icon and the reason `size` is optional.
            sizes: icon
                .size
                .map_or_else(|| "any".to_owned(), |size| format!("{size}x{size}")),
            // From the path alone: a `?v=2` cache-buster is part of the request
            // and not of the file, and reading it as the extension would tell a
            // launcher the image is an unknown binary and have it skip the icon.
            r#type: Mime::of(Tail::of(&icon.src).path).to_string(),
            // The default is what a manifest means by an absent `purpose`, so
            // writing it would only restate the spec.
            purpose: (icon.purpose != IconPurpose::default()).then(|| icon.purpose.name()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DisplayMode, LanguageConfig};

    fn configured() -> Config {
        let mut config = Config {
            site: Some("Baudelaire".into()),
            ..Config::default()
        };
        config.generate.manifest = ManifestConfig {
            enabled: true,
            display: DisplayMode::Minimal,
            theme: Some("#101014".into()),
            icons: vec![
                IconConfig {
                    src: "/icons/app-192.png".into(),
                    size: Some(192),
                    purpose: IconPurpose::Any,
                },
                IconConfig {
                    src: "/icons/app.svg".into(),
                    size: None,
                    purpose: IconPurpose::Maskable,
                },
            ],
            ..ManifestConfig::default()
        };
        config
    }

    #[test]
    fn derives_the_members_the_config_leaves_out() {
        let config = configured();
        let json = serde_json::to_value(Document::new(&config, &config.lang)).unwrap();
        assert_eq!(json["name"], "Baudelaire");
        assert_eq!(json["start_url"], "/");
        assert_eq!(json["scope"], "/");
        // `minimal` is one word in config; the member it writes is not.
        assert_eq!(json["display"], "minimal-ui");
        assert_eq!(json["theme_color"], "#101014");
        // Nothing said, nothing written: an absent member and an empty one mean
        // different things to a browser.
        assert!(json.get("short_name").is_none());
        assert!(json.get("background_color").is_none());
    }

    /// A vector icon scales, so it declares `any` rather than a made-up size,
    /// and only a non-default purpose is spelled out.
    #[test]
    fn an_icon_carries_its_size_type_and_purpose() {
        let config = configured();
        let json = serde_json::to_value(Document::new(&config, &config.lang)).unwrap();
        let icons = json["icons"].as_array().unwrap();
        assert_eq!(icons[0]["sizes"], "192x192");
        assert_eq!(icons[0]["type"], "image/png");
        assert!(icons[0].get("purpose").is_none());
        assert_eq!(icons[1]["sizes"], "any");
        assert_eq!(icons[1]["type"], "image/svg+xml");
        assert_eq!(icons[1]["purpose"], "maskable");
    }

    /// A non-default language gets its own manifest, launching into its own
    /// scope: installing from `/fr/` must not open the English site.
    #[test]
    fn a_language_launches_into_its_own_scope() {
        let mut config = configured();
        config.url = Some("https://example.com/docs".into());
        config.languages = vec![("fr".into(), LanguageConfig::default())];
        let json = serde_json::to_value(Document::new(&config, "fr")).unwrap();
        assert_eq!(json["lang"], "fr");
        assert_eq!(json["start_url"], "/docs/fr/");
        assert_eq!(json["scope"], "/docs/fr/");
        // ...and the icons a base path serves are under it too.
        assert_eq!(json["icons"][0]["src"], "/docs/icons/app-192.png");
        assert_eq!(
            ManifestConfig::url(&config, "fr"),
            "/docs/fr/manifest.webmanifest"
        );
    }

    /// An authored `start`/`scope` is localized like the defaults are. Written
    /// once and shared verbatim, a `start_url` would sit outside the `scope`
    /// every other language localized, which is a manifest a browser refuses.
    #[test]
    fn an_authored_start_and_scope_are_localized_too() {
        let mut config = configured();
        config.languages = vec![("fr".into(), LanguageConfig::default())];
        config.generate.manifest.start = Some("/home/".into());
        config.generate.manifest.scope = Some("/app/".into());

        let english = serde_json::to_value(Document::new(&config, "en")).unwrap();
        assert_eq!(english["start_url"], "/home/");
        assert_eq!(english["scope"], "/app/");

        let french = serde_json::to_value(Document::new(&config, "fr")).unwrap();
        assert_eq!(french["start_url"], "/fr/home/");
        assert_eq!(french["scope"], "/fr/app/");
    }
}
