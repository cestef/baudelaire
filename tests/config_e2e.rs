mod common;

use baudelaire::config::Config;
use common::Site;

fn fixture_config() -> &'static str {
    r#"
        site "Fixture Site"
        url "https://fixture.test"
        lang "en"

        paths {
          content "content"
          dist "public"
        }

        clean #true

        typst {
          inputs {
            env "test"
          }
          features "+html"
        }

        collections {
          posts "posts/**/*.typ" sort="date" reverse=#true
          notes "notes/**/*.typ" sort="order"
        }

        taxonomies {
          tags index=#true
        }

        output {
          html {
            pretty #true
          }
        }

        serve {
          port 1821
          bind "127.0.0.1"
          open #false
          watch #true
        }

        profiles {
          dev {
            url "http://localhost:1821"
            future #true
          }
          prod {
            output {
              html {
                pretty #false
              }
            }
          }
        }
    "#
}

#[test]
fn loads_full_config() {
    let sb = Site::new();
    sb.write("config.kdl", fixture_config());
    let text = sb.read("config.kdl");
    let cfg = Config::parse(&text).expect("parse");
    assert_eq!(cfg.site.as_deref(), Some("Fixture Site"));
    assert_eq!(cfg.url.as_deref(), Some("https://fixture.test"));
    assert_eq!(cfg.lang, "en");
    assert!(cfg.clean);
    assert_eq!(cfg.inputs, vec![("env".into(), "test".into())]);
    assert_eq!(cfg.features, vec!["html".to_owned()]);
    assert_eq!(cfg.collections.len(), 2);
    assert_eq!(cfg.taxonomies.len(), 1);
    assert!(cfg.html.pretty);
    assert!(!cfg.serve.open);
    assert_eq!(cfg.profiles.len(), 2);
}

#[test]
fn profile_dev_overrides() {
    let sb = Site::new();
    sb.write("config.kdl", fixture_config());
    let text = sb.read("config.kdl");
    let cfg = Config::parse(&text).expect("parse");
    let dev = cfg.with_profile("dev").expect("profile dev");
    assert_eq!(dev.url.as_deref(), Some("http://localhost:1821"));
    assert!(dev.future);
}

#[test]
fn profile_prod_overrides_nested() {
    let sb = Site::new();
    sb.write("config.kdl", fixture_config());
    let text = sb.read("config.kdl");
    let cfg = Config::parse(&text).expect("parse");
    let prod = cfg.with_profile("prod").expect("profile prod");
    assert!(!prod.html.pretty);
}

#[test]
fn minimal_config_builds() {
    let sb = Site::new();
    sb.write("config.kdl", r#"site "Minimal""#);
    let text = sb.read("config.kdl");
    let cfg = Config::parse(&text).expect("parse");
    assert_eq!(cfg.site.as_deref(), Some("Minimal"));
    assert_eq!(cfg.dist, std::path::PathBuf::from("public"));
    assert_eq!(cfg.content, std::path::PathBuf::from("content"));
}

#[test]
fn empty_config_uses_defaults() {
    let sb = Site::new();
    sb.write("config.kdl", "");
    let text = sb.read("config.kdl");
    let cfg = Config::parse(&text).expect("parse");
    assert_eq!(cfg.lang, "en");
    assert_eq!(cfg.serve.port, 1821);
    assert!(cfg.cache.incremental);
}

#[test]
fn malformed_config_errors_with_message() {
    let sb = Site::new();
    sb.write("config.kdl", "serve {\n  port \"not-a-number\"\n}\n");
    let text = sb.read("config.kdl");
    let err = Config::parse(&text).expect_err("should fail");
    assert!(err.to_string().contains("expected integer"));
}
