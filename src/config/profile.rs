use kdl::KdlDocument;
use miette::SourceSpan;

use crate::config::Config;
use crate::config::dispatch::Keys;
use crate::config::node::NodeExt;
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
        assert!(prod.html.meta, "meta inherited from base");
    }

    /// Fill-in-place has to hold all the way down, not just one level: the
    /// grouped tree makes `a { b { c .. } }` the common shape (a `dev` profile
    /// flipping `content { draft { build } }`), and an override there must leave
    /// every untouched sibling at *each* level alone.
    #[test]
    fn profile_overrides_three_levels_deep() {
        let cfg = parse(
            r#"
            content {
              index "_index"
              draft { build #false; suffix ".wip" }
            }
            assets {
              minify #true
              images { lazy #false; responsive { quality 70; widths 320 640 } }
            }
            profiles {
              dev {
                content { draft { build #true } }
                assets { images { responsive { quality 55 } } }
              }
            }
        "#,
        );
        let dev = cfg.with_profile("dev").expect("profile exists");
        assert!(dev.content.draft.build, "overridden three levels down");
        assert_eq!(dev.content.draft.suffix, ".wip", "sibling key inherited");
        assert_eq!(dev.content.index.as_deref(), Some("_index"), "uncle key");
        let responsive = &dev.assets.images.responsive;
        assert_eq!(responsive.quality, 55, "overridden four levels down");
        assert_eq!(responsive.widths, vec![320, 640], "sibling list inherited");
        assert!(!dev.assets.images.lazy, "sibling section inherited");
        assert!(dev.assets.minify, "grandparent sibling inherited");
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
        let cfg = parse("profiles {\n  dev {\n    profiles { inner { prune #true } }\n  }\n}\n");
        let err = cfg.with_profile("dev").expect_err("nested profiles");
        assert!(
            err.to_string()
                .contains("`profiles` cannot be nested inside a profile"),
            "{err}"
        );
    }

    #[test]
    fn profile_overlay_error_points_at_original_config_text() {
        let text = "site \"x\"\nprofiles {\n  dev {\n    prune \"yes\"\n  }\n}\n";
        let err = parse(text).with_profile("dev").expect_err("bad boolean");
        let rendered = format!("{:?}", miette::Report::from(err));
        assert!(
            rendered.contains("expected boolean, got string"),
            "{rendered}"
        );
        // The label must excerpt the original config.kdl, not a re-serialized
        // profile subtree with mismatched offsets.
        assert!(rendered.contains("prune \"yes\""), "{rendered}");
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
