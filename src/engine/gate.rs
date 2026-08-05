//! What this binary cannot do, and what this site's own config withholds.
//!
//! Two tables, and nothing else. [`GATES`] is the single source of truth for
//! feature degradation: every `#[cfg(feature)]` in the tree removes capability
//! in silence, and a row here is what turns that into a diagnostic. [`INERT`]
//! is its counterpart for settings gated by *each other* rather than by the
//! build: a site can ask for something its own config withholds, and be told.
//!
//! Both are read once, by `Engine::new`, before anything else looks at the
//! config: the whole build has to agree on one answer rather than disagreeing
//! file by file.

use crate::config::{Config, SearchConfig};
use crate::error::warning::{FeatureMissing, SettingInert};

/// One optional capability, the config that asks for it, and what a binary
/// built without it does instead.
///
/// The single source of truth for feature degradation. Every `#[cfg(feature)]`
/// in the tree removes capability in silence: a `.css` file is copied verbatim,
/// a card is never drawn, an image is never re-encoded, and the build is green
/// either way. One row here is what turns that into a diagnostic, instead of a
/// warning hand-written at each site that happens to notice.
pub(super) struct Gate {
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
    /// files it owns. Content-hashing renames files that other files name, so a
    /// build that lost such a rewriter serves a stylesheet still naming its
    /// assets by their pre-hash spelling: 404s out of a green build. Losing one
    /// turns `assets { fingerprint }` off for the whole build instead.
    rewrites: bool,
}

const GATES: &[Gate] = &[
    Gate {
        cargo: "markdown",
        compiled: cfg!(feature = "markdown"),
        setting: "content { markdown }",
        // Asked by writing a `.md` page, not by writing the node: markdown is on
        // by default and a `.md` file under `content/` is simply a page, so the
        // documented way to want it is to say nothing at all. Reading the node
        // instead, a slim binary on such a site discovered no markdown pages,
        // warned about nothing, and let the prune delete the HTML a
        // full-featured build had written for them. A site that says
        // `markdown #false` has decided against the capability, and warning that
        // it is missing would answer a question it did not ask.
        asked: Gate::markdown,
        effect: "`.md` files under `content/` are not pages, and are left where they lie",
        rewrites: false,
    },
    Gate {
        cargo: "css",
        compiled: cfg!(feature = "css"),
        setting: "assets { minify }",
        asked: |config| config.assets.minify,
        effect: "stylesheets are copied unminified",
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
        cargo: "js",
        compiled: cfg!(feature = "js"),
        setting: "assets { bundle }",
        asked: |config| config.assets.bundle,
        effect: "JavaScript is copied verbatim, its imports unresolved and its output unminified",
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
    // Unlike its neighbours this names a capability of `deploy`, not of the
    // build. It sits here anyway so the table stays the single place a gated
    // capability is declared, and so a build warns about a destination it will
    // not be able to reach rather than waiting for the deploy to say so.
    Gate {
        cargo: "ssh",
        compiled: cfg!(feature = "ssh"),
        setting: "deploy { ssh }",
        asked: |config| config.deploy.ssh.is_some(),
        effect: "the SSH destination is skipped",
        rewrites: false,
    },
    // Announcing is a command, but it also shapes the *build*: a pinned `did`
    // emits a `.well-known` record and a per-page backlink. Both vanish here,
    // which a site that pinned a `did` very much wants to hear about, since
    // their absence is what makes a publication unverifiable.
    Gate {
        cargo: "announce",
        compiled: cfg!(feature = "announce"),
        setting: "announce { standard }",
        asked: |config| config.announce.standard.is_some(),
        effect: "no verification artifacts are emitted and `announce` is unavailable",
        rewrites: false,
    },
];

/// One config setting that does nothing unless another is also set.
///
/// The counterpart of [`Gate`] for settings gated by *each other* rather than
/// by a cargo feature, and the single source of truth for that class the same
/// way. Each of these was accepted by the parser, changed nothing about the
/// build, and said nothing: a `stopwords` list tuning an index format the site
/// does not emit, a `terms` feed over taxonomies that publish no term page.
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
    // A `bundle { }` block naming neither a collection nor the site binds no
    // pages, so it wrote no document and said nothing about it.
    Inert {
        setting: "generate { pdf { bundle } }",
        asked: |config| config.generate.pdf.bundle.present,
        needs: "a `collections` list or `site`",
        met: |config| config.generate.pdf.bundle.enabled(),
        effect: "no bundled document is written",
        help: "name the collections to bind (`collections \"guide\"`), or set `site #true` for the whole site",
    },
    // `assets { minify }` has no row of its own. It minifies stylesheets on its
    // own, which is the whole of what a site with no JavaScript asked for, and
    // this table is for settings that produce *nothing*. The row that used to
    // sit here fired on every such site, including the `docs` starter, and the
    // only way to silence it was to turn on a bundler the site had no use for.
    // The bundle-only half of `minify` is documented on the asset pipeline page.
    //
    // The bundler is the only thing that reads a tsconfig: with `bundle` off,
    // scripts are copied verbatim and the pinned file is never opened.
    Inert {
        setting: "assets { tsconfig }",
        asked: |config| config.assets.tsconfig.is_some(),
        needs: "assets { bundle }",
        met: |config| config.assets.bundle,
        effect: "TypeScript and JSX are copied verbatim, untransformed",
        help: "turn on `assets { bundle }`, or drop the `tsconfig` path",
    },
    // Term feeds sit beside term listing pages, and a taxonomy publishes none
    // unless it asks: `terms` alone wrote no files and warned about nothing.
    Inert {
        setting: "generate { feed { terms } }",
        asked: |config| config.generate.feed.terms,
        needs: "a taxonomy with `listing`",
        met: |config| config.content.taxonomies.iter().any(|(_, t)| t.listing),
        effect: "no per-term feed is written",
        help: "set `listing` on the taxonomy whose terms should carry a feed",
    },
    // A collection feed is written beside the collection's index and takes that
    // index as its home, so a collection with none has nowhere to put one: the
    // feed would advertise a `<link>` no page answers.
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
        // One row cannot name which collection, so it says "that collection"
        // rather than claiming none was written: with two asking and one
        // paginated, the paginated one does get its feed.
        effect: "that collection's feed is not written",
        help: "add `paginate { }` to the collection, which is the page its feed points at",
    },
    // Both kinds of subsidiary feed ride on the formats the site writes, and
    // naming none turns feeds off wholesale: the collection key sits far from
    // `generate { feed { } }`, so asking there and nowhere else was silence.
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
    // Both tune the prebuilt inverted index and reach no other format, so a
    // site on `formats \"json\"` tuned nothing at all.
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
    // The verification artifacts are the point of pinning a `did`: without one
    // there is nothing to reference, and `verify` defaults on, so a site could
    // ask for both and get neither in silence.
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
    // An `integrity` pins a digest to a URL. Where the URL is not
    // content-addressed, the file behind it changes while the pages naming it
    // stay cached, and every one of them then blocks the very stylesheet it
    // asked for. Stamping nothing is the safe half of that bargain.
    // The policy is written into `_headers` and nowhere else: it is a header,
    // and a static build has no other way to send one. Without that file the
    // whole block is a paragraph of config that produces nothing.
    Inert {
        setting: "security { csp }",
        asked: |config| config.security.csp.enabled,
        needs: "generate { headers }",
        met: |config| config.generate.headers,
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
    /// Whether this site has markdown pages to lose: the capability is on (it
    /// is, unless the site turned it off) and at least one `.md` file sits under
    /// the content tree.
    ///
    /// A filesystem probe rather than a config read, because a markdown page
    /// asks for nothing: it is a file. It costs one walk of the content tree,
    /// and only in a binary that cannot do markdown, since [`Gate::resolve`]
    /// tests `compiled` first and never reaches this in the full-featured build.
    /// An unreadable tree answers no: discovery reports that, with the path.
    fn markdown(config: &Config) -> bool {
        config.content.markdown.enabled
            && crate::fs::Walk::new(&config.paths.content)
                .files()
                .unwrap_or_default()
                .iter()
                .any(|path| path.extension().is_some_and(|ext| ext == "md"))
    }

    /// Walk the table once against a site's config: name every capability it
    /// asked for that this binary lacks, and turn `assets { fingerprint }` off
    /// when what would have kept it honest is missing.
    ///
    /// Turning it off rather than refusing the build is the same bargain every
    /// other gate strikes: the capability goes, the site still stands. It is
    /// applied here, before anything reads the config, so the whole build (the
    /// pipeline's renames, the render pass's rewrites, the cache fingerprint)
    /// agrees on one answer rather than disagreeing file by file.
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

    /// Content-hashing renames a file that other files name. Without the `css`
    /// feature nothing rewrites the `url()` inside a stylesheet, so the sheet is
    /// served naming assets that no longer exist: a green build and a 404 site.
    /// Fingerprinting is turned off for the whole build instead, and said so.
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

    /// The walk only ever fires on a setting the site opted into, so a config
    /// that asks for nothing optional is untouched whatever this binary lacks.
    #[test]
    fn a_site_asking_for_nothing_optional_is_untouched() {
        let (resolved, gaps) = Gate::resolve(config(""));
        assert!(!resolved.assets.fingerprint);
        assert!(gaps.is_empty());
    }

    /// Markdown is asked for by writing a page, not by writing a config node:
    /// `.md` files are pages by default, so the documented way to want them is
    /// to say nothing at all. Reading the node instead, a binary without the
    /// feature discovered no markdown pages on such a site, said nothing, and
    /// let the prune delete the HTML a full-featured build had written.
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
        // A site with no markdown, and no config about it, is asking for nothing.
        assert!(!Gate::markdown(&site("")));
        std::fs::write(content.join("posts/a.typ"), "= a\n").expect("typst page");
        assert!(!Gate::markdown(&site("")));
        // One `.md` file is the whole of the ask, config or no config.
        std::fs::write(content.join("posts/b.md"), "# b\n").expect("markdown page");
        assert!(Gate::markdown(&site("")));
        // ...and a site that turned the capability off has decided against it,
        // whatever is lying in its content tree.
        assert!(!Gate::markdown(&site("content { markdown #false }")));
    }

    /// A gate names the config that asks for it, and codes read as identity, so
    /// two rows describing the same setting would report the same gap twice.
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

    /// Every row fires on the config it describes, and stops once the setting
    /// it depends on is there. Written as the pair, because a warning that
    /// cannot be silenced by doing what it asks is worse than none.
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

    /// The counterpart of [`every_gate_names_a_distinct_setting`].
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
