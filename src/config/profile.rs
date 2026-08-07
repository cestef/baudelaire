use kdl::KdlDocument;
use miette::SourceSpan;

use crate::config::Config;
use crate::config::dispatch::Keys;
use crate::config::node::NodeExt;
use crate::config::value::Structural;
use crate::error::{ConfigError, ConfigErrorKind, Result};

impl Config {
    pub fn with_profile(mut self, name: &str) -> Result<Self> {
        // take the partials out instead of cloning the subtree; restored after overlay
        let profiles = std::mem::take(&mut self.profiles);
        let partial = profiles
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, doc)| doc)
            .ok_or_else(|| ConfigError::missing_profile(name, &profiles))?;
        // overlay errors report against the *original* config text: retained nodes
        // carry spans into it, so labels point at the real config.kdl lines
        let text = self.source.clone();
        for node in partial.nodes() {
            if node.name().value() == Self::PROFILES {
                return Err(ConfigError::nested_profiles(&text, NodeExt::span(node)).into());
            }
            self.overlay(&text, node)?;
        }
        self.profiles = profiles;
        self.profile = Some(name.to_owned());
        Ok(self)
    }

    /// Apply every configured profile once and throw the result away, so a
    /// profile is checked by the parse that read it.
    ///
    /// A profile block is retained as raw KDL and dispatched only when
    /// selected, which left an unselected one entirely unchecked: a typo in
    /// `profiles { dev { content { draft #true } } }` parsed green, reloaded
    /// green under `serve`, and set nothing whenever `--profile dev` was
    /// finally passed. The check is the real overlay rather than a separate
    /// walk of the tables, so what it accepts and what selection accepts cannot
    /// drift.
    ///
    /// What it does *not* check is values: the check runs as a
    /// [`Structural`] pass, so an unset `${VAR}` with no `:-default` stands in
    /// as a placeholder here and is a hard error only where the profile is
    /// really selected. A `prod` profile naming `${S3_BUCKET}` is the
    /// documented shape, and demanding it of every local `build` failed every
    /// build on a machine that has no production secrets.
    pub(super) fn check(&self) -> Result<()> {
        let _shape = Structural::begin();
        for (name, _) in &self.profiles {
            self.clone().with_profile(name)?;
        }
        Ok(())
    }
}

impl ConfigError {
    /// A profile name that matches nothing in `profiles { .. }`, its help
    /// listing (and nearest-matching) the names that are configured.
    pub fn missing_profile(name: &str, profiles: &[(String, KdlDocument)]) -> Self {
        let names: Vec<&str> = profiles.iter().map(|(n, _)| n.as_str()).collect();
        let help = if names.is_empty() {
            "no profiles are configured; add a `profiles { .. }` block to config.kdl".to_owned()
        } else {
            Keys(&names).help(name, "profiles")
        };
        Self::at(
            "",
            ConfigErrorKind::MissingProfile {
                name: name.to_owned(),
                help,
            },
            SourceSpan::new(0.into(), 0),
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;

    fn parse(text: &str) -> Config {
        Config::parse(text).expect("should parse")
    }

    #[test]
    fn profile_overrides_url() {
        let cfg = parse(
            r#"
            url "https://example.net"
            profiles {
              dev {
                url "http://localhost:1821"
              }
            }
        "#,
        );
        let dev = cfg.with_profile("dev").expect("profile exists");
        assert_eq!(dev.url.as_deref(), Some("http://localhost:1821"));
    }

    #[test]
    fn profile_inherits_absent_keys() {
        let cfg = parse(
            r#"
            site "Baudelaire"
            lang "fr"
            profiles {
              prod {
                content { future #true }
              }
            }
        "#,
        );
        let prod = cfg.with_profile("prod").expect("profile exists");
        assert_eq!(prod.site.as_deref(), Some("Baudelaire"));
        assert_eq!(prod.lang, "fr");
        assert!(prod.content.future);
    }

    #[test]
    fn profile_overrides_nested_html() {
        let cfg = parse(
            r"
            html {
              pretty #true
            }
            profiles {
              prod {
                html {
                  pretty #false
                }
              }
            }
        ",
        );
        let prod = cfg.with_profile("prod").expect("profile exists");
        assert!(!prod.html.pretty);
    }

    #[test]
    fn profile_override_preserves_sibling_fields() {
        // overriding one field of a nested section must inherit the base's others, not reset them
        let cfg = parse(
            r"
            html { pretty #true; embed #true; meta #true }
            profiles {
              prod {
                html { pretty #false }
              }
            }
        ",
        );
        let prod = cfg.with_profile("prod").expect("profile exists");
        assert!(!prod.html.pretty, "pretty overridden");
        assert!(prod.html.embed, "embed inherited from base");
        assert!(prod.html.meta.enabled, "meta inherited from base");
    }

    /// Fill-in-place has to hold all the way down, not just one level: the
    /// grouped tree makes `a { b { c .. } }` the common shape (a `dev` profile
    /// flipping `content { drafts { build } }`), and an override there must leave
    /// every untouched sibling at *each* level alone.
    #[test]
    fn profile_overrides_three_levels_deep() {
        let cfg = parse(
            r#"
            content {
              index "_index"
              drafts { build #false; suffix ".wip" }
            }
            assets {
              minify #true
              images { lazy #false; responsive { quality 70; widths 320 640 } }
            }
            profiles {
              dev {
                content { drafts #true }
                assets { images { responsive { quality 55 } } }
              }
            }
        "#,
        );
        let dev = cfg.with_profile("dev").expect("profile exists");
        assert!(dev.content.drafts.build, "overridden three levels down");
        assert_eq!(dev.content.drafts.suffix, ".wip", "sibling key inherited");
        assert_eq!(dev.content.index.as_deref(), Some("_index"), "uncle key");
        let responsive = &dev.assets.images.responsive;
        assert_eq!(responsive.quality, 55, "overridden four levels down");
        assert_eq!(responsive.widths, vec![320, 640], "sibling list inherited");
        assert!(!dev.assets.images.lazy, "sibling section inherited");
        assert!(dev.assets.minify.css, "grandparent sibling inherited");
    }

    #[test]
    fn profile_overrides_serve() {
        let cfg = parse(
            r"
            serve {
              port 1821
            }
            profiles {
              ci {
                serve {
                port 9000
              }
              }
            }
        ",
        );
        let ci = cfg.with_profile("ci").expect("profile exists");
        assert_eq!(ci.serve.port, 9000);
    }

    #[test]
    fn profile_not_found_errors() {
        let cfg = parse("site \"x\"");
        let err = cfg
            .with_profile("nope")
            .expect_err("no profiles configured");
        let rendered = format!("{:?}", miette::Report::from(err));
        assert!(rendered.contains("profile `nope` not found"), "{rendered}");
        assert!(
            rendered.contains("no profiles are configured"),
            "{rendered}"
        );
    }

    #[test]
    fn profile_not_found_help_lists_configured_names() {
        let cfg = parse("profiles {\n  dev { prune #true }\n  prod { prune #false }\n}\n");
        let err = cfg.with_profile("prd").expect_err("unknown profile");
        let rendered = format!("{:?}", miette::Report::from(err));
        assert!(rendered.contains("profile `prd` not found"), "{rendered}");
        assert!(rendered.contains("did you mean `prod`?"), "{rendered}");
        assert!(
            rendered.contains("valid profiles: `dev`, `prod`"),
            "{rendered}"
        );
    }

    #[test]
    fn profile_rejects_nested_profiles() {
        let err =
            Config::parse("profiles {\n  dev {\n    profiles { inner { prune #true } }\n  }\n}\n")
                .expect_err("nested profiles");
        assert!(
            err.to_string()
                .contains("`profiles` cannot be nested inside a profile"),
            "{err}"
        );
    }

    #[test]
    fn profile_overlay_error_points_at_original_config_text() {
        let text = "site \"x\"\nprofiles {\n  dev {\n    prune \"yes\"\n  }\n}\n";
        let err = Config::parse(text).expect_err("bad boolean");
        let rendered = format!("{:?}", miette::Report::from(err));
        assert!(
            rendered.contains("expected boolean, got string"),
            "{rendered}"
        );
        // The label must excerpt the original config.kdl, not a re-serialized
        // profile subtree with mismatched offsets.
        assert!(rendered.contains("prune \"yes\""), "{rendered}");
    }

    /// A profile nobody selected is still config, and used to be dispatched
    /// only on selection: a key no scope has parsed green, survived a `serve`
    /// reload, and then quietly configured nothing.
    #[test]
    fn an_unselected_profile_is_checked_at_parse() {
        let err = Config::parse("profiles {\n  dev {\n    content { draft #true }\n  }\n}\n")
            .expect_err("unknown key inside an unselected profile");
        let rendered = format!("{:?}", miette::Report::from(err));
        assert!(
            rendered.contains("unknown config key `draft`"),
            "{rendered}"
        );
        assert!(rendered.contains("did you mean `drafts`?"), "{rendered}");
    }

    /// The check reads a profile for its *structure*, so a `${VAR}` naming a
    /// secret the selecting build will supply is nobody's problem until then.
    /// It used to be everyone's: a `prod` profile with `${S3_BUCKET}` in it
    /// failed every `build` on every machine that had no production secrets,
    /// including the one the docs recommend writing.
    #[test]
    fn an_unset_variable_in_an_unselected_profile_is_not_a_build_failure() {
        let text = "site \"T\"\nprofiles {\n  prod {\n    url \"${BAUDELAIRE_TEST_UNSET_URL}\"\n    deploy { s3 { bucket \"${BAUDELAIRE_TEST_UNSET_BUCKET}\"; endpoint \"${BAUDELAIRE_TEST_UNSET_ENDPOINT}\" } }\n  }\n}\n";
        let cfg = parse(text);
        assert_eq!(cfg.site.as_deref(), Some("T"), "the base is untouched");
        assert_eq!(cfg.url, None, "and no placeholder leaked into it");

        // Selecting it is the other path, and there the value is about to be
        // used: it still errors, naming the variable.
        let err = cfg.with_profile("prod").expect_err("unset at selection");
        let rendered = format!("{:?}", miette::Report::from(err));
        assert!(
            rendered.contains("environment variable `BAUDELAIRE_TEST_UNSET_URL` is not set"),
            "{rendered}"
        );
    }

    /// Structure is still structure: the placeholder answers what a value *is*,
    /// never which keys are written, so a typo inside a profile that also names
    /// an unset variable is still caught at parse.
    #[test]
    fn a_typo_beside_an_unset_variable_is_still_caught() {
        let err = Config::parse(
            "profiles {\n  prod {\n    url \"${BAUDELAIRE_TEST_UNSET_URL}\"\n    content { draft #true }\n  }\n}\n",
        )
        .expect_err("unknown key");
        let rendered = format!("{:?}", miette::Report::from(err));
        assert!(rendered.contains("did you mean `drafts`?"), "{rendered}");
    }

    /// The check applies each profile to a *copy*: parsing a config must not
    /// leave the base narrowed by the last profile that happened to be checked.
    #[test]
    fn checking_a_profile_does_not_narrow_the_base_config() {
        let cfg =
            parse("content { future #false }\nprofiles {\n  dev { content { future #true } }\n}\n");
        assert!(!cfg.content.future, "base untouched by the check");
        assert_eq!(cfg.profile, None);
        assert_eq!(cfg.profiles.len(), 1);
    }

    #[test]
    fn profile_partials_survive_application() {
        let cfg = parse("profiles {\n  dev { content { future #true } }\n}\n");
        let dev = cfg.with_profile("dev").expect("profile exists");
        assert_eq!(dev.profiles.len(), 1, "partials are restored after overlay");
    }

    #[test]
    fn profile_future_flag() {
        let cfg = parse(
            r"
            content { future #false }
            profiles {
              dev {
                content { future #true }
              }
            }
        ",
        );
        let dev = cfg.clone().with_profile("dev").expect("profile exists");
        assert!(dev.content.future);
        assert!(!cfg.content.future);
    }
}
