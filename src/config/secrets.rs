//! The environment variables that carry a credential, and everything that
//! reads one.

/// The variables a credential is passed in.
///
/// `${VAR}` expansion refuses every one of them, so no config value can carry a
/// secret into the built site, a diagnostic, or `--json` output. Each reader
/// names its variable from here rather than spelling it a second time.
pub struct Secrets;

impl Secrets {
    /// The S3 secret access key.
    pub const S3_SECRET_KEY: &'static str = "AWS_SECRET_ACCESS_KEY";

    /// The S3 session token, which signs alongside a temporary key.
    pub const S3_SESSION_TOKEN: &'static str = "AWS_SESSION_TOKEN";

    /// The password or passphrase a deploy over SSH authenticates with.
    pub const SSH_PASSWORD: &'static str = "BAUDELAIRE_SSH_PASSWORD";

    /// The app password an announce authenticates to its PDS with.
    pub const ATPROTO_PASSWORD: &'static str = "BAUDELAIRE_ATPROTO_PASSWORD";

    /// Every one of them, ungated: a name is refused whether or not this build
    /// carries the feature that reads it.
    pub const ALL: &'static [&'static str] = &[
        Self::S3_SECRET_KEY,
        Self::S3_SESSION_TOKEN,
        Self::SSH_PASSWORD,
        Self::ATPROTO_PASSWORD,
    ];

    /// Whether `name` is one of them.
    pub fn carried_by(name: &str) -> bool {
        Self::ALL.contains(&name)
    }
}

#[cfg(test)]
mod tests {
    use super::Secrets;

    #[test]
    fn every_secret_is_named_once() {
        let mut names = Secrets::ALL.to_vec();
        names.sort_unstable();
        let len = names.len();
        names.dedup();
        assert_eq!(names.len(), len, "{:?}", Secrets::ALL);
    }
}
