use crate::config::Config;
use crate::error::{ConfigError, ConfigErrorKind, Result};
use miette::SourceSpan;

impl Config {
    pub fn with_profile(mut self, name: &str) -> Result<Self> {
        let partial = self
            .profiles
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, doc)| doc.clone())
            .ok_or_else(|| ConfigError::missing_profile(name))?;
        let text = partial.to_string();
        for node in partial.nodes() {
            self.overlay(&text, node)?;
        }
        self.profile = Some(name.to_owned());
        Ok(self)
    }
}

impl ConfigError {
    pub fn missing_profile(name: &str) -> ConfigError {
        ConfigError::at(
            "",
            ConfigErrorKind::BadValue {
                detail: format!("profile `{name}` not found in config.kdl"),
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
                url "http://localhost:3000"
              }
            }
        "#,
        );
        let dev = cfg.with_profile("dev").expect("profile exists");
        assert_eq!(dev.url.as_deref(), Some("http://localhost:3000"));
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
    fn profile_overrides_serve() {
        let cfg = parse(
            r#"
            serve {
              port 3000
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
        assert!(cfg.with_profile("nope").is_err());
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
