//! A minimal, blocking XRPC client for the `com.atproto.*` methods a publisher
//! needs: authenticate, upload a blob, put/list/delete records.

use serde::Serialize;
use serde_json::{Value, json};

use crate::error::AnnounceError;
use crate::mime::Mime;
use crate::ui::markup;

use super::id::{Did, Nsid, Rkey};

/// A blob reference returned by `uploadBlob`, opaque so it round-trips into the
/// owning record unmodified.
#[derive(Debug, Clone, Serialize)]
pub struct Blob(Value);

/// A read-only handle to a repository on a PDS; every read it offers is a
/// public XRPC call needing no auth.
pub struct Repo {
    agent: ureq::Agent,
    /// The PDS/entryway base, e.g. `https://bsky.social`, without a trailing
    /// `/`.
    host: String,
    did: Did,
}

impl Repo {
    /// An agent that surfaces 4xx/5xx as ordinary responses, not transport
    /// errors, so an XRPC error body can be read and surfaced instead of
    /// swallowed.
    fn agent() -> ureq::Agent {
        crate::remote::Http::transferring("announce", crate::remote::Status::Read)
    }

    /// Resolve `identifier`, a handle or a DID, to a repo reader on `host`.
    pub fn resolve(host: &str, identifier: &str) -> Result<Self, AnnounceError> {
        let host = host.trim_end_matches('/').to_owned();
        let agent = Self::agent();
        let did = if identifier.starts_with("did:") {
            Did::new(identifier)
        } else {
            const NSID: &str = "com.atproto.identity.resolveHandle";
            let url = format!("{host}/xrpc/{NSID}");
            let mut resp = agent.get(&url).query("handle", identifier).call()?;
            Did::new(resp.json::<Value>(NSID)?.field("did")?)
        };
        Ok(Self { agent, host, did })
    }

    pub fn did(&self) -> &Did {
        &self.did
    }

    /// Every record key currently in `collection`, following pagination, and
    /// erroring rather than returning a short list.
    pub fn list_rkeys(&self, collection: Nsid) -> Result<Vec<Rkey>, AnnounceError> {
        const NSID: &str = "com.atproto.repo.listRecords";
        const MAX_PAGES: usize = 10_000;
        let mut rkeys = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut req = self
                .agent
                .get(self.xrpc(NSID))
                .query("repo", self.did.as_str())
                .query("collection", collection.as_str())
                .query("limit", "100");
            if let Some(cursor) = &cursor {
                req = req.query("cursor", cursor);
            }
            let mut resp = req.call()?;
            let value: Value = resp.json(NSID)?;
            for record in value
                .get("records")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(rkey) = record
                    .get("uri")
                    .and_then(Value::as_str)
                    .and_then(|uri| uri.rsplit('/').next())
                {
                    rkeys.push(Rkey::parsed(rkey));
                }
            }
            match value.get("cursor").and_then(Value::as_str) {
                Some(next) if !next.is_empty() => cursor = Some(next.to_owned()),
                _ => return Ok(rkeys),
            }
        }
        Err(AnnounceError::Pagination {
            nsid: NSID.to_owned(),
            pages: MAX_PAGES,
        })
    }

    fn xrpc(&self, nsid: &str) -> String {
        format!("{}/xrpc/{nsid}", self.host)
    }
}

/// An authenticated session against a single PDS host: a [`Repo`] plus the
/// bearer token that unlocks the record-mutating calls.
pub struct Session {
    repo: Repo,
    access: String,
}

impl Session {
    /// Authenticate to `host` with a handle (or DID) and app password.
    pub fn login(host: &str, identifier: &str, password: &str) -> Result<Self, AnnounceError> {
        let agent = Repo::agent();
        let host = host.trim_end_matches('/').to_owned();
        let url = format!("{host}/xrpc/com.atproto.server.createSession");
        let body = json!({ "identifier": identifier, "password": password });
        let mut resp = agent.post(&url).send_json(&body)?;
        let value: Value = resp.json("com.atproto.server.createSession")?;
        Ok(Self {
            repo: Repo {
                agent,
                host,
                did: Did::new(value.field("did")?),
            },
            access: value.field("accessJwt")?,
        })
    }

    /// The authenticated repository DID, which overrides any configured guess.
    pub fn did(&self) -> &Did {
        self.repo.did()
    }

    pub fn repo(&self) -> &Repo {
        &self.repo
    }

    pub fn upload_blob(&self, bytes: &[u8], mime: Mime) -> Result<Blob, AnnounceError> {
        const NSID: &str = "com.atproto.repo.uploadBlob";
        let mut resp = self
            .repo
            .agent
            .post(self.repo.xrpc(NSID))
            .header("Authorization", self.bearer())
            .header("Content-Type", mime.to_string())
            .send(bytes)?;
        let value: Value = resp.json(NSID)?;
        value.get("blob").cloned().map(Blob).ok_or_else(|| {
            AnnounceError::xrpc(NSID, resp.status().as_u16(), "response had no `blob`")
        })
    }

    /// Create or replace a record at `collection/rkey`.
    pub fn put_record(
        &self,
        collection: Nsid,
        rkey: &Rkey,
        record: &impl Serialize,
    ) -> Result<(), AnnounceError> {
        const NSID: &str = "com.atproto.repo.putRecord";
        self.post(
            NSID,
            &json!({
                "repo": self.did().as_str(),
                "collection": collection.as_str(),
                "rkey": rkey.as_str(),
                "record": record,
            }),
        )
    }

    pub fn delete_record(&self, collection: Nsid, rkey: &Rkey) -> Result<(), AnnounceError> {
        const NSID: &str = "com.atproto.repo.deleteRecord";
        self.post(
            NSID,
            &json!({
                "repo": self.did().as_str(),
                "collection": collection.as_str(),
                "rkey": rkey.as_str(),
            }),
        )
    }

    /// POST a JSON `body` to `nsid` with bearer auth, discarding the response.
    fn post(&self, nsid: &str, body: &Value) -> Result<(), AnnounceError> {
        let mut resp = self
            .repo
            .agent
            .post(self.repo.xrpc(nsid))
            .header("Authorization", self.bearer())
            .send_json(body)?;
        resp.json::<Value>(nsid).map(drop)
    }

    fn bearer(&self) -> String {
        format!("Bearer {}", self.access)
    }
}

/// Read an XRPC response body as `T`, turning a non-2xx status into a
/// [`AnnounceError`] carrying the PDS's own error body.
trait ResponseExt {
    fn json<T: serde::de::DeserializeOwned>(&mut self, nsid: &str) -> Result<T, AnnounceError>;
}

impl ResponseExt for ureq::http::Response<ureq::Body> {
    fn json<T: serde::de::DeserializeOwned>(&mut self, nsid: &str) -> Result<T, AnnounceError> {
        let status = self.status().as_u16();
        if !(200..300).contains(&status) {
            let message = self.body_mut().read_to_string().unwrap_or_default();
            return Err(AnnounceError::xrpc(nsid, status, message));
        }
        Ok(self.body_mut().read_json::<T>()?)
    }
}

/// A required string field of a JSON object, else an auth error naming it.
trait ValueExt {
    fn field(&self, key: &str) -> Result<String, AnnounceError>;
}

impl ValueExt for Value {
    fn field(&self, key: &str) -> Result<String, AnnounceError> {
        self.get(key)
            .and_then(Self::as_str)
            .map(str::to_owned)
            .ok_or_else(|| AnnounceError::auth(markup!("session response had no `{}`", key)))
    }
}
