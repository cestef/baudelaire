//! `announce { }` and `deploy { }`: the optional backends.

use super::parse;
#[test]
fn announce_standard_block_enables_backend_with_defaults() {
    let cfg = parse("announce {\n  standard {\n    handle \"me.bsky.social\"\n  }\n}\n");
    let standard = cfg.announce.standard.expect("standard backend configured");
    assert_eq!(standard.handle, "me.bsky.social");
    assert_eq!(standard.pds, "https://bsky.social");
    assert!(standard.discover);
    assert!(standard.icon.is_none());
}

#[test]
fn announce_unset_leaves_no_backend() {
    assert!(parse("").announce.standard.is_none());
}

#[test]
fn announce_standard_did_and_verify_toggles() {
    let cfg = parse(
        "announce {\n  standard {\n    handle \"me.example\"\n    did \"did:plc:abc\"\n    verify {\n      links #false\n    }\n  }\n}\n",
    );
    let standard = cfg.announce.standard.expect("configured");
    assert_eq!(standard.did.as_deref(), Some("did:plc:abc"));
    // toggled off explicitly; the untouched sibling keeps its default
    assert!(!standard.verify.links);
    assert!(standard.verify.wellknown);
}

#[test]
fn deploy_s3_block_enables_backend_with_defaults() {
    let cfg = parse(
        "deploy {\n  s3 {\n    bucket \"my-site\"\n    endpoint \"https://acct.r2.cloudflarestorage.com\"\n    region \"auto\"\n  }\n}\n",
    );
    let s3 = cfg.deploy.s3.expect("s3 backend configured");
    assert_eq!(s3.bucket, "my-site");
    assert_eq!(
        s3.endpoint.as_deref(),
        Some("https://acct.r2.cloudflarestorage.com")
    );
    assert_eq!(s3.region(), "auto");
    assert_eq!(s3.prefix, "");
    assert!(s3.delete, "delete defaults on");
}

/// An unstated region follows the target. A custom `endpoint` is not AWS, and
/// signing such a request as `us-east-1` is a 403 whose body never mentions the
/// region; AWS itself keeps its own default.
#[test]
fn an_unstated_s3_region_follows_the_endpoint() {
    let r2 =
        parse("deploy { s3 { bucket \"b\"; endpoint \"https://acct.r2.cloudflarestorage.com\" } }");
    assert_eq!(r2.deploy.s3.unwrap().region(), "auto");
    let aws = parse("deploy { s3 { bucket \"b\" } }");
    assert_eq!(aws.deploy.s3.unwrap().region(), "us-east-1");
}

#[test]
fn deploy_ssh_block_enables_backend_with_defaults() {
    let cfg = parse(
        "deploy {\n  ssh {\n    host \"example.com\"\n    path \"/var/www/site\"\n    user \"deploy\"\n  }\n}\n",
    );
    let ssh = cfg.deploy.ssh.expect("ssh backend configured");
    assert_eq!(ssh.host, "example.com");
    assert_eq!(ssh.path, "/var/www/site");
    assert_eq!(ssh.user.as_deref(), Some("deploy"));
    assert_eq!(ssh.port, 22, "port defaults to 22");
    assert!(ssh.key.is_none());
    assert!(ssh.strict, "host-key verification defaults on");
    assert!(ssh.delete, "delete defaults on");
}

#[test]
fn deploy_unset_leaves_no_backend() {
    let deploy = parse("").deploy;
    assert!(deploy.s3.is_none());
    assert!(deploy.ssh.is_none());
}
