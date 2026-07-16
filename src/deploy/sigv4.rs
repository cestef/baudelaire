//! AWS Signature Version 4, hand-rolled on `sha2`/`hmac` so it can be pinned to
//! AWS's own test vectors (see the tests). The S3 deploy backend signs its
//! `ureq` requests with a [`Signer`]; nothing here is async or S3-specific.
//!
//! A [`Request`] carries its `payload_hash` ready-made (the hex SHA-256 of the
//! body, or `"UNSIGNED-PAYLOAD"`) rather than the body itself, so [`Signer::sign`]
//! is a pure function of its inputs — which is what lets the vectors match to the
//! byte.

use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};

/// A request to sign. `uri` is the already-encoded absolute path; `query` is the
/// canonical (sorted, encoded) query string, or empty; `headers` are extra
/// signed headers as `(name, value)` — `host` and `x-amz-date` are added for you.
pub struct Request<'a> {
    pub method: &'a str,
    pub host: &'a str,
    pub uri: &'a str,
    pub query: &'a str,
    pub headers: &'a [(&'a str, &'a str)],
    pub payload_hash: &'a str,
}

/// Credentials and the scope — region, service, time — a signature binds to.
pub struct Signer<'a> {
    pub access_key: &'a str,
    pub secret_key: &'a str,
    pub region: &'a str,
    pub service: &'a str,
    /// Request time as `YYYYMMDDTHHMMSSZ`, UTC.
    pub timestamp: &'a str,
}

impl Signer<'_> {
    /// The `Authorization` header value for `req`.
    pub fn sign(&self, req: &Request) -> String {
        let (canonical, headers) = self.canonical(req);
        let to_sign = join(&[
            "AWS4-HMAC-SHA256",
            self.timestamp,
            &self.scope(),
            &sha256_hex(canonical.as_bytes()),
        ]);
        let signature = hex(&hmac(&self.key(), to_sign.as_bytes()));
        format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={headers}, Signature={signature}",
            self.access_key,
            self.scope(),
        )
    }

    /// The canonical request and its `;`-joined signed-header list. `host` and
    /// `x-amz-date` are folded in; values are whitespace-collapsed and the set is
    /// sorted by lowercase name.
    fn canonical(&self, req: &Request) -> (String, String) {
        let mut headers = vec![("host", req.host.to_owned()), ("x-amz-date", self.timestamp.to_owned())];
        headers.extend(req.headers.iter().map(|(name, value)| (*name, collapse(value))));
        headers.sort_by_key(|(name, _)| name.to_lowercase());

        let names = headers.iter().map(|(name, _)| name.to_lowercase()).collect::<Vec<_>>().join(";");
        let rows: String =
            headers.iter().map(|(name, value)| format!("{}:{value}\n", name.to_lowercase())).collect();
        let canonical =
            join(&[req.method, req.uri, req.query, &rows, &names, req.payload_hash]);
        (canonical, names)
    }

    /// The credential scope: `YYYYMMDD/region/service/aws4_request`.
    fn scope(&self) -> String {
        format!("{}/{}/{}/aws4_request", self.date(), self.region, self.service)
    }

    /// The signing key: HMAC-chained from the secret through date, region, and
    /// service to the `aws4_request` terminator.
    fn key(&self) -> Vec<u8> {
        let seed = format!("AWS4{}", self.secret_key);
        [self.date(), self.region, self.service, "aws4_request"]
            .into_iter()
            .fold(seed.into_bytes(), |key, part| hmac(&key, part.as_bytes()))
    }

    /// The `YYYYMMDD` date, sliced from the timestamp.
    fn date(&self) -> &str {
        &self.timestamp[..8]
    }
}

/// The lowercase hex SHA-256 of `data` — the value S3 wants in
/// `x-amz-content-sha256`, and the digest used throughout signing.
pub fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

/// HMAC-SHA256 of `message` under `key`.
fn hmac(key: &[u8], message: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(message);
    mac.finalize().into_bytes().to_vec()
}

/// Lowercase, zero-padded hex.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// Newline-join — the separator every SigV4 string uses.
fn join(parts: &[&str]) -> String {
    parts.join("\n")
}

/// Trim and collapse internal whitespace runs to single spaces, per the spec.
fn collapse(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixed credentials AWS's SigV4 test suite signs every case with.
    /// <https://github.com/awslabs/aws-c-auth/tree/main/tests/aws-signing-test-suite/v4>
    fn signer() -> Signer<'static> {
        Signer {
            access_key: "AKIDEXAMPLE",
            secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            region: "us-east-1",
            service: "service",
            timestamp: "20150830T123600Z",
        }
    }

    /// SHA-256 of the empty body, the payload hash every GET case uses.
    const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn signature(auth: &str) -> &str {
        auth.rsplit_once("Signature=").expect("authorization carries a signature").1
    }

    #[test]
    fn sha256_of_empty_is_the_known_constant() {
        assert_eq!(sha256_hex(b""), EMPTY);
    }

    #[test]
    fn get_vanilla_matches_the_suite() {
        let req = Request {
            method: "GET",
            host: "example.amazonaws.com",
            uri: "/",
            query: "",
            headers: &[],
            payload_hash: EMPTY,
        };
        let (canonical, names) = signer().canonical(&req);
        assert_eq!(names, "host;x-amz-date");
        assert_eq!(
            canonical,
            "GET\n/\n\nhost:example.amazonaws.com\nx-amz-date:20150830T123600Z\n\nhost;x-amz-date\n"
                .to_owned()
                + EMPTY
        );
        // the suite publishes the hash of the canonical request (its string-to-sign line)
        assert_eq!(
            sha256_hex(canonical.as_bytes()),
            "bb579772317eb040ac9ed261061d46c1f17a8133879d6129b6e1c25292927e63"
        );
        assert_eq!(
            signature(&signer().sign(&req)),
            "5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
        );
    }

    #[test]
    fn get_vanilla_query_matches_the_suite() {
        // the caller supplies the already-sorted canonical query.
        let req = Request {
            method: "GET",
            host: "example.amazonaws.com",
            uri: "/",
            query: "Param1=value1&Param2=value2",
            headers: &[],
            payload_hash: EMPTY,
        };
        assert_eq!(
            signature(&signer().sign(&req)),
            "b97d918cfa904a5beff61c982a1b6f458b799221646efd99d3219ec94cdf2500"
        );
    }

    #[test]
    fn get_header_value_trim_matches_the_suite() {
        // mixed-case names, out of order, and a value with collapsible runs.
        let req = Request {
            method: "GET",
            host: "example.amazonaws.com",
            uri: "/",
            query: "",
            headers: &[("My-Header1", "value1"), ("My-Header2", "\"a   b   c\"")],
            payload_hash: EMPTY,
        };
        let (_, names) = signer().canonical(&req);
        assert_eq!(names, "host;my-header1;my-header2;x-amz-date");
        assert_eq!(
            signature(&signer().sign(&req)),
            "acc3ed3afb60bb290fc8d2dd0098b9911fcaa05412b367055dee359757a9c736"
        );
    }

    #[test]
    fn full_authorization_header_is_well_formed() {
        let auth = signer().sign(&Request {
            method: "GET",
            host: "example.amazonaws.com",
            uri: "/",
            query: "",
            headers: &[],
            payload_hash: EMPTY,
        });
        assert_eq!(
            auth,
            "AWS4-HMAC-SHA256 \
             Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, \
             SignedHeaders=host;x-amz-date, \
             Signature=5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
        );
    }

    #[test]
    fn hex_is_zero_padded_lowercase() {
        assert_eq!(hex(&[0x00, 0x0f, 0xff, 0xa0]), "000fffa0");
    }

    #[test]
    fn collapse_trims_and_squeezes_runs() {
        assert_eq!(collapse("  a   b  c "), "a b c");
    }
}
