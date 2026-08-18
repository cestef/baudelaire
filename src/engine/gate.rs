//! What this binary cannot do ([`GATES`]) and what this site's own config
//! withholds ([`INERT`]), both read once by `Engine::new` so the whole build
//! agrees on one answer.

use crate::config::{BundleFormat, Config, SearchConfig};
use crate::error::warning::{FeatureMissing, SettingInert};

/// One optional capability, the config that asks for it, and what a binary
/// built without it does instead: the single source of truth for feature
/// degradation.
pub(crate) struct Gate {
    /// The cargo feature that compiles the capability in.
    cargo: &'static str,
    /// Whether this binary has it. Spelled per row because `cfg!` takes a
    /// feature name literally and so cannot be derived from `cargo`.
    compiled: bool,
    /// The config that asks for it, as the author writes it in `config.kdl`.
    setting: &'static str,
    /// Whether this site asked.
    asked: fn(&Config) -> bool,
    /// What the build produces instead.
    effect: &'static str,
    /// Whether this capability is what rewrites the references *inside* the
    /// files it owns; losing one turns `assets { fingerprint }` off for the
    /// whole build, since a hashed asset would be named by its old spelling.
    rewrites: bool,
}

const GATES: &[Gate] = &[
    Gate {
        cargo: "markdown",
        compiled: cfg!(feature = "markdown"),
        setting: "content { markdown }",
        asked: Gate::markdown,
        effect: "`.md` files under `content/` are not pages, and are left where they lie",
        rewrites: false,
    },
    Gate {
        cargo: "css",
        compiled: cfg!(feature = "css"),
        setting: "assets { minify }",
        asked: |config| config.assets.minify.css(),
        effect: "stylesheets are copied unminified",
        rewrites: false,
    },
    Gate {
        cargo: "css",
        compiled: cfg!(feature = "css"),
        setting: "assets { targets }",
        asked: |config| config.assets.targets.any(),
        effect: "stylesheets are copied as written, un-downlevelled and unprefixed",
        rewrites: false,
    },
    Gate {
        cargo: "css",
        compiled: cfg!(feature = "css"),
        setting: "assets { fingerprint }",
        asked: |config| config.assets.fingerprint,
        effect: "asset filenames are left unhashed, since the `url()` and `@import` references inside stylesheets cannot be rewritten to match",
        rewrites: true,
    },
    Gate {
        cargo: "css",
        compiled: cfg!(feature = "css"),
        setting: "assets { sourcemap }",
        asked: |config| config.assets.sourcemap.styles.wanted(),
        effect: "no source map is written beside a processed stylesheet",
        rewrites: false,
    },
    Gate {
        cargo: "sass",
        compiled: cfg!(feature = "sass"),
        setting: "a `.scss` or `.sass` file under the asset tree",
        asked: Gate::sass,
        effect: "Sass sources are left where they lie, and no stylesheet is written from them",
        rewrites: false,
    },
    Gate {
        cargo: "js",
        compiled: cfg!(feature = "js"),
        setting: "assets { bundle }",
        asked: |config| config.assets.bundle,
        effect: "JavaScript is copied verbatim with its imports unresolved, and TypeScript is not published at all",
        rewrites: false,
    },
    Gate {
        cargo: "tailwind",
        compiled: cfg!(feature = "tailwind"),
        setting: "assets { tailwind }",
        asked: |config| config.assets.tailwind.enabled,
        effect: "no utility stylesheet is generated, and the pages that link one get no rules for the classes they carry",
        rewrites: false,
    },
    Gate {
        cargo: "images",
        compiled: cfg!(feature = "images"),
        setting: "assets { images { optimize } }",
        asked: |config| config.assets.images.optimize.any(),
        effect: "PNG and JPEG assets are copied unoptimized",
        rewrites: false,
    },
    Gate {
        cargo: "images",
        compiled: cfg!(feature = "images"),
        setting: "assets { images { responsive } }",
        asked: |config| config.assets.images.responsive.enabled,
        effect: "no width variants are written and no `srcset` is emitted",
        rewrites: false,
    },
    Gate {
        cargo: "cards",
        compiled: cfg!(feature = "cards"),
        setting: "generate { cards }",
        asked: |config| config.generate.cards.enabled,
        effect: "no social card is rendered",
        rewrites: false,
    },
    Gate {
        cargo: "pdf",
        compiled: cfg!(feature = "pdf"),
        setting: "generate { pdf }",
        asked: |config| config.generate.pdf.enabled(),
        effect: "no PDF is written beside a page, and nothing links to one",
        rewrites: false,
    },
    Gate {
        cargo: "pdf",
        compiled: cfg!(feature = "pdf"),
        setting: "generate { bundles { formats \"pdf\" } }",
        asked: |config| Gate::bundles(config, BundleFormat::Pdf),
        effect: "no PDF is written for that bundle",
        rewrites: false,
    },
    Gate {
        cargo: "epub",
        compiled: cfg!(feature = "epub"),
        setting: "generate { bundles { formats \"epub\" } }",
        asked: |config| Gate::bundles(config, BundleFormat::Epub),
        effect: "no EPUB is written for that bundle",
        rewrites: false,
    },
    Gate {
        cargo: "ssh",
        compiled: cfg!(feature = "ssh"),
        setting: Gate::SSH,
        asked: |config| config.deploy.ssh.is_some(),
        effect: "the SSH destination is skipped",
        rewrites: false,
    },
    Gate {
        cargo: "announce",
        compiled: cfg!(feature = "announce"),
        setting: "announce { standard }",
        asked: |config| config.announce.standard.is_some(),
        effect: "no verification artifacts are emitted and `announce` is unavailable",
        rewrites: false,
    },
];

/// One config setting that does nothing unless another is also set: the
/// counterpart of [`Gate`] for settings gated by each other rather than by a
/// cargo feature.
pub(super) struct Inert {
    /// The setting that was asked for, as the author writes it in `config.kdl`.
    setting: &'static str,
    /// Whether this site asked.
    asked: fn(&Config) -> bool,
    /// What it depends on.
    needs: &'static str,
    /// Whether that dependency is satisfied.
    met: fn(&Config) -> bool,
    /// What the build produces instead.
    effect: &'static str,
    /// How to make it take effect, or how to stop asking.
    help: &'static str,
}

const INERT: &[Inert] = &[
    Inert {
        setting: "generate { bundles }",
        asked: |config| !config.generate.bundles.is_empty(),
        needs: "a `collections` list or `site` on each bundle",
        met: |config| config.generate.bundles.iter().all(|(_, b)| b.enabled()),
        effect: "that bundle binds no pages, so no document is written",
        help: "name the collections to bind (`collections \"guide\"`), or set `site #true` for the whole site",
    },
    Inert {
        setting: "assets { tsconfig }",
        asked: |config| config.assets.tsconfig.is_some(),
        needs: "assets { bundle }",
        met: |config| config.assets.bundle,
        effect: "TypeScript and JSX are copied verbatim, untransformed",
        help: "turn on `assets { bundle }`, or drop the `tsconfig` path",
    },
    Inert {
        setting: "generate { feed { terms } }",
        asked: |config| config.generate.feed.terms,
        needs: "a taxonomy with `listing`",
        met: |config| config.content.taxonomies.iter().any(|(_, t)| t.listing),
        effect: "no per-term feed is written",
        help: "set `listing` on the taxonomy whose terms should carry a feed",
    },
    Inert {
        setting: "content { collections { feed } }",
        asked: |config| config.content.collections.iter().any(|(_, c)| c.feed),
        needs: "a `paginate` block on that collection",
        met: |config| {
            config
                .content
                .collections
                .iter()
                .all(|(_, c)| !c.feed || c.paginate.enabled)
        },
        effect: "that collection's feed is not written",
        help: "add `paginate { }` to the collection, which is the page its feed points at",
    },
    Inert {
        setting: "a `feed` beside a collection or a term",
        asked: |config| {
            config.generate.feed.terms || config.content.collections.iter().any(|(_, c)| c.feed)
        },
        needs: "generate { feed { formats } }",
        met: |config| !config.generate.feed.formats.is_empty(),
        effect: "no feed of any kind is written",
        help: "name the formats to write (`formats \"rss\"`), or drop the `feed` that asked",
    },
    Inert {
        setting: "generate { search { stopwords } }",
        asked: |config| !config.generate.search.stopwords.is_empty(),
        needs: "generate { search { formats \"inverted\" } }",
        met: |config| config.generate.search.inverted(),
        effect: "the flat `json` index carries every token",
        help: "add `inverted` to `formats`, or drop the stopwords",
    },
    Inert {
        setting: "generate { search { minimum } }",
        asked: |config| config.generate.search.min_length != SearchConfig::default().min_length,
        needs: "generate { search { formats \"inverted\" } }",
        met: |config| config.generate.search.inverted(),
        effect: "the flat `json` index carries every token",
        help: "add `inverted` to `formats`, or drop the minimum",
    },
    Inert {
        setting: "announce { standard { verify } }",
        asked: |config| {
            config
                .announce
                .standard
                .as_ref()
                .is_some_and(|s| s.verify.wellknown || s.verify.links)
        },
        needs: "announce { standard { did } }",
        met: |config| {
            config
                .announce
                .standard
                .as_ref()
                .is_some_and(|s| s.did.is_some())
        },
        effect: "no `.well-known` record and no per-page backlink are emitted, so the publication cannot be verified",
        help: "pin the account's `did`, or turn `verify` off",
    },
    Inert {
        setting: "a `redirect` old path carrying `*`",
        asked: |config| {
            config
                .redirect
                .iter()
                .any(|(old, _)| crate::config::Config::wildcard(old))
        },
        needs: "generate { redirects }",
        met: |config| config.generate.redirects,
        effect: "the pattern is dropped, since a wildcard cannot be an HTML stub",
        help: "turn on `generate { redirects }`, or write the old paths out one by one",
    },
    Inert {
        setting: "a `redirect` naming a `status`",
        asked: |config| config.redirect.iter().any(|(_, rule)| rule.needs_rules()),
        needs: "generate { redirects }",
        met: |config| config.generate.redirects,
        effect: "the HTML stub forwards the browser, and no host is told the status",
        help: "turn on `generate { redirects }`, or drop the `status` and let it be a permanent move",
    },
    Inert {
        setting: "security { csp }",
        asked: |config| config.security.csp.enabled,
        needs: "generate { headers }",
        met: |config| config.generate.headers.enabled,
        effect: "no policy is written, since `_headers` is the file it goes in",
        help: "turn on `generate { headers }`, or drop the `csp { }` block",
    },
    Inert {
        setting: "security { sri }",
        asked: |config| config.security.sri,
        needs: "assets { fingerprint }",
        met: |config| config.assets.fingerprint,
        effect: "no `integrity` attribute is stamped, since a digest pinned to a name that can change under it blocks the file it was meant to protect",
        help: "turn on `assets { fingerprint }`, which is what makes an asset URL name one exact file",
    },
];

impl Inert {
    /// Walk the table once against a site's config: name every setting that
    /// asked for something the config it sits in cannot deliver.
    pub(super) fn resolve(config: &Config) -> Vec<SettingInert> {
        INERT
            .iter()
            .filter(|inert| (inert.asked)(config) && !(inert.met)(config))
            .map(SettingInert::from)
            .collect()
    }
}

impl From<&Inert> for SettingInert {
    fn from(inert: &Inert) -> Self {
        Self {
            setting: inert.setting,
            needs: inert.needs,
            effect: inert.effect,
            help: inert.help,
        }
    }
}

impl Gate {
    /// The setting the SSH destination is asked for by, named because `deploy`
    /// needs it too and never constructs an `Engine`.
    pub(crate) const SSH: &'static str = "deploy { ssh }";

    /// The gap this binary has at `setting`, or `None` when it has the
    /// capability. For a caller that cannot reach [`Gate::resolve`] and has
    /// already established that the site asked, so `asked` is not consulted.
    pub(crate) fn missing_for(setting: &str) -> Option<FeatureMissing> {
        GATES
            .iter()
            .find(|gate| gate.setting == setting && !gate.compiled)
            .map(FeatureMissing::from)
    }

    /// Whether this site has markdown pages to lose: the capability is on and
    /// at least one `.md` file sits under the content tree. A filesystem probe
    /// rather than a config read, because a markdown page asks for nothing: it
    /// is a file.
    /// Whether any bundle asks for `format`, read off the *defaulted* list so
    /// the row still fires for the `pdf` a bundle takes by writing nothing.
    fn bundles(config: &Config, format: BundleFormat) -> bool {
        config
            .generate
            .bundles
            .iter()
            .any(|(_, bundle)| bundle.enabled() && bundle.formats().contains(&format))
    }

    fn markdown(config: &Config) -> bool {
        config.content.markdown.enabled
            && crate::fs::Walk::new(&config.paths.content)
                .files()
                .unwrap_or_default()
                .iter()
                .any(|path| Config::has_ext(path, Config::MARKDOWN))
    }

    /// Whether this site has a Sass source to lose, probed the way
    /// [`Gate::markdown`] probes the content tree. The theme's tree is not
    /// walked: a theme states the binary it needs.
    fn sass(config: &Config) -> bool {
        crate::fs::Walk::new(&config.paths.assets)
            .files()
            .unwrap_or_default()
            .iter()
            .any(|path| Config::SASS.iter().any(|ext| Config::has_ext(path, ext)))
    }

    /// Walk the table once against a site's config: name every capability it
    /// asked for that this binary lacks, and turn `assets { fingerprint }` off
    /// when what would have kept it honest is missing. Applied before anything
    /// reads the config, so the whole build agrees on one answer.
    pub(super) fn resolve(mut config: Config) -> (Config, Vec<FeatureMissing>) {
        let missing: Vec<&Self> = GATES
            .iter()
            .filter(|gate| !gate.compiled && (gate.asked)(&config))
            .collect();
        if missing.iter().any(|gate| gate.rewrites) {
            config.assets.fingerprint = false;
        }
        (
            config,
            missing.into_iter().map(FeatureMissing::from).collect(),
        )
    }
}

impl From<&Gate> for FeatureMissing {
    fn from(gate: &Gate) -> Self {
        Self {
            setting: gate.setting,
            cargo: gate.cargo,
            effect: gate.effect,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GATES, Gate, INERT, Inert};
    use crate::config::Config;

    fn config(text: &str) -> Config {
        Config::parse(text).expect("should parse")
    }

    #[test]
    fn fingerprinting_without_the_stylesheet_rewriter_is_turned_off_and_reported() {
        let asked = config("assets { fingerprint #true }");
        assert!(asked.assets.fingerprint, "the site asked for it");
        let (resolved, gaps) = Gate::resolve(asked);
        assert_eq!(
            resolved.assets.fingerprint,
            cfg!(feature = "css"),
            "kept where stylesheets can be rewritten, dropped where they cannot"
        );
        assert_eq!(
            gaps.iter()
                .any(|gap| gap.setting == "assets { fingerprint }" && gap.cargo == "css"),
            !cfg!(feature = "css"),
            "turning a setting off is never silent"
        );
    }

    /// A feature with no row degrades silently, which is what `epub` did until
    /// it got one: every capability a build can lack is either gated or named
    /// here as gating no setting of its own.
    #[test]
    fn every_optional_capability_owns_a_gate_row() {
        /// Features that gate no config setting: one supplies fonts the binary
        /// falls back from on its own, the other only decides whether `theme`
        /// can fetch, which its own command reports.
        const UNGATED: [&str; 2] = ["embedded-fonts", "themes"];

        for (feature, _) in crate::version::Version::FEATURES {
            let gated = GATES.iter().any(|gate| gate.cargo == *feature);
            assert_eq!(
                gated,
                !UNGATED.contains(feature),
                "`{feature}` has {} gate row",
                if gated { "a" } else { "no" }
            );
        }
    }

    /// A bundle takes `pdf` by writing no `formats` at all, so a row reading
    /// the stored list rather than the defaulted one never fires.
    #[test]
    fn a_bundle_asks_for_the_format_it_defaults_to() {
        let cfg = config("generate {\n  bundles {\n    guide { collections \"guide\" }\n  }\n}");
        assert!(
            Gate::bundles(&cfg, crate::config::BundleFormat::Pdf),
            "a bundle with no `formats` still asks for a PDF"
        );
        assert!(!Gate::bundles(&cfg, crate::config::BundleFormat::Epub));

        let epub = config(
            "generate {\n  bundles {\n    guide { collections \"guide\"; formats \"epub\" }\n  }\n}",
        );
        assert!(Gate::bundles(&epub, crate::config::BundleFormat::Epub));
        assert!(!Gate::bundles(&epub, crate::config::BundleFormat::Pdf));

        let unbound = config("generate {\n  bundles {\n    guide { }\n  }\n}");
        assert!(
            !Gate::bundles(&unbound, crate::config::BundleFormat::Pdf),
            "a bundle binding nothing asks for nothing"
        );
    }

    #[test]
    fn a_site_asking_for_nothing_optional_is_untouched() {
        let (resolved, gaps) = Gate::resolve(config(""));
        assert!(!resolved.assets.fingerprint);
        assert!(gaps.is_empty());
    }

    #[test]
    fn markdown_is_asked_for_by_a_page_on_disk_not_by_a_config_node() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let content = tmp.path().join("content");
        std::fs::create_dir_all(content.join("posts")).expect("content tree");
        let site = |text: &str| {
            let mut config = config(text);
            config.paths.content.clone_from(&content);
            config
        };
        assert!(!Gate::markdown(&site("")));
        std::fs::write(content.join("posts/a.typ"), "= a\n").expect("typst page");
        assert!(!Gate::markdown(&site("")));
        std::fs::write(content.join("posts/b.md"), "# b\n").expect("markdown page");
        assert!(Gate::markdown(&site("")));
        assert!(!Gate::markdown(&site("content { markdown #false }")));
    }

    #[test]
    fn a_gap_is_looked_up_by_the_setting_that_asks_for_it() {
        let ssh = Gate::missing_for(Gate::SSH);
        assert_eq!(ssh.is_some(), !cfg!(feature = "ssh"));
        if let Some(gap) = ssh {
            assert_eq!(gap.cargo, "ssh");
            assert_eq!(gap.setting, Gate::SSH);
        }
        assert!(Gate::missing_for("deploy { carrier-pigeon }").is_none());
    }

    #[test]
    fn every_gate_names_a_distinct_setting() {
        for (i, gate) in GATES.iter().enumerate() {
            assert!(
                !GATES[i + 1..].iter().any(|o| o.setting == gate.setting),
                "`{}` is claimed by two gates",
                gate.setting
            );
        }
    }

    #[test]
    fn each_inert_setting_reports_until_its_dependency_is_set() {
        let cases = [
            (
                "generate { feed { terms } }",
                "generate { feed { formats \"rss\"; terms #true } }",
                "generate { feed { formats \"rss\"; terms #true } }\ncontent { taxonomies { tags listing=#true } }",
            ),
            (
                "generate { search { stopwords } }",
                "generate { search { formats \"json\"; stopwords \"the\" } }",
                "generate { search { formats \"inverted\"; stopwords \"the\" } }",
            ),
            (
                "generate { search { minimum } }",
                "generate { search { formats \"json\"; minimum 4 } }",
                "generate { search { formats \"inverted\"; minimum 4 } }",
            ),
            (
                "announce { standard { verify } }",
                "announce { standard { handle \"a.example\" } }",
                "announce { standard { handle \"a.example\"; did \"did:plc:x\" } }",
            ),
            (
                "a `redirect` old path carrying `*`",
                "redirect {\n  \"/latest/*\" \"/:splat\"\n}",
                "generate {\n  redirects #true\n}\nredirect {\n  \"/latest/*\" \"/:splat\"\n}",
            ),
        ];
        for (setting, asked, satisfied) in cases {
            let named = |text| {
                Inert::resolve(&config(text))
                    .iter()
                    .any(|i| i.setting == setting)
            };
            assert!(named(asked), "`{setting}` did not report on `{asked}`");
            assert!(
                !named(satisfied),
                "`{setting}` still reports once its dependency is set"
            );
        }
    }

    #[test]
    fn every_inert_row_names_a_distinct_setting() {
        for (i, inert) in INERT.iter().enumerate() {
            assert!(
                !INERT[i + 1..].iter().any(|o| o.setting == inert.setting),
                "`{}` is claimed by two rows",
                inert.setting
            );
        }
    }
}
