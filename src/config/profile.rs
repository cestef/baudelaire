use kdl::KdlDocument;
use miette::SourceSpan;

use crate::config::Config;
use crate::config::dispatch::Keys;
use crate::config::parse::NodeExt;
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
        // overlay errors report against the *original* config text — retained nodes
        // carry spans into it, so labels point at the real config.kdl lines
        let text = self.source.clone();
        for node in partial.nodes() {
            if node.name().value() == "profiles" {
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
    pub fn missing_profile(name: &str, profiles: &[(String, KdlDocument)]) -> ConfigError {
        let names: Vec<&str> = profiles.iter().map(|(n, _)| n.as_str()).collect();
        let help = if names.is_empty() {
            "no profiles are configured; add a `profiles { .. }` block to config.kdl".to_owned()
        } else {
            Keys(&names).help(name, "profiles")
        };
        ConfigError::at(
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
                future #true
              }
            }
        "#,
        );
        let prod = cfg.with_profile("prod").expect("profile exists");
        assert_eq!(prod.site.as_deref(), Some("Baudelaire"));
        assert_eq!(prod.lang, "fr");
        assert!(prod.future);
    }

    #[test]
    fn profile_overrides_nested_html() {
        let cfg = parse(
            r#"
            output {
              html {
                pretty #true
              }
            }
            profiles {
              prod {
                output {
                  html {
                    pretty #false
                  }
                }
              }
            }
        "#,
        );
        let prod = cfg.with_profile("prod").expect("profile exists");
        assert!(!prod.html.pretty);
    }

    #[test]
    fn profile_override_preserves_sibling_fields() {
        // overriding one field of a nested section must inherit the base's others, not reset them
        let cfg = parse(
            r#"
            output {
              html { pretty #true; embed #true; meta #true }
            }
            profiles {
              prod {
                output { html { pretty #false } }
              }
            }
        "#,
        );
        let prod = cfg.with_profile("prod").expect("profile exists");
        assert!(!prod.html.pretty, "pretty overridden");
        assert!(prod.html.embed, "embed inherited from base");
        assert!(prod.html.meta, "meta inherited from base");
    }

    #[test]
    fn profile_overrides_serve() {
        let cfg = parse(
            r#"
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
        "#,
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
        let cfg = parse("profiles {\n  dev { future #true }\n  prod { future #false }\n}\n");
        let err = cfg.with_profile("prd").expect_err("unknown profile");
        let rendered = format!("{:?}", miette::Report::from(err));
        assert!(rendered.contains("profile `prd` not found"), "{rendered}");
        assert!(rendered.contains("did you mean `prod`?"), "{rendered}");
        assert!(rendered.contains("valid profiles: dev, prod"), "{rendered}");
    }

    #[test]
    fn profile_rejects_nested_profiles() {
        let cfg = parse("profiles {\n  dev {\n    profiles { inner { future #true } }\n  }\n}\n");
        let err = cfg.with_profile("dev").expect_err("nested profiles");
        assert!(
            err.to_string()
                .contains("`profiles` cannot be nested inside a profile"),
            "{err}"
        );
    }

    #[test]
    fn profile_overlay_error_points_at_original_config_text() {
        let text = "site \"x\"\nprofiles {\n  dev {\n    output {\n      clean \"yes\"\n    }\n  }\n}\n";
        let err = parse(text).with_profile("dev").expect_err("bad boolean");
        let rendered = format!("{:?}", miette::Report::from(err));
        assert!(
            rendered.contains("expected boolean, got string"),
            "{rendered}"
        );
        // The label must excerpt the original config.kdl, not a re-serialized
        // profile subtree with mismatched offsets.
        assert!(rendered.contains("clean \"yes\""), "{rendered}");
    }

    #[test]
    fn profile_partials_survive_application() {
        let cfg = parse("profiles {\n  dev { future #true }\n}\n");
        let dev = cfg.with_profile("dev").expect("profile exists");
        assert_eq!(dev.profiles.len(), 1, "partials are restored after overlay");
    }

    #[test]
    fn profile_future_flag() {
        let cfg = parse(
            r#"
            future #false
            profiles {
              dev {
                future #true
              }
            }
        "#,
        );
        let dev = cfg.clone().with_profile("dev").expect("profile exists");
        assert!(dev.future);
        assert!(!cfg.future);
    }
}
