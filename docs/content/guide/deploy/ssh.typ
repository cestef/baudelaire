#let frontmatter = (
  title: "SSH & SFTP",
  order: 9,
)
#import "/templates/theme.typ": callout

`baudelaire deploy` also targets any host reachable over SSH, transferring files
with SFTP. Add an `ssh` block instead of (or alongside) an
#link("s3.typ")[`s3`] one:

```kdl
deploy {
  ssh {
    host "example.com"
    path "/var/www/site"
    key "~/.ssh/id_ed25519"
  }
}
```

/ `host`: server hostname or IP. Required.
/ `path`: absolute remote directory the build is mirrored into. Required.
/ `port` (`22`): SSH port.
/ `user`: user to authenticate as. Defaults to `$USER`.
/ `key`: path to a private key (absolute, `~`-relative, or under the project
  root). Unset tries the ssh-agent, then a password.
/ `strict` (`#true`): verify the server's host key against `~/.ssh/known_hosts`:
  learning an unseen host on first connect and refusing a changed key. Set
  `#false` to accept any key (`StrictHostKeyChecking=no`).
/ `delete` (`#true`): remove remote files under `path` that the build no longer
  produces.

== Authentication

Baudelaire tries, in order: the configured `key` (supply its passphrase, if any,
like any other secret below); the *ssh-agent* at `$SSH_AUTH_SOCK`, offering each
identity it holds; then a *password* resolved, in order of precedence, from
stdin (`--secret -`, which wins when both are present), the
`BAUDELAIRE_SSH_PASSWORD` environment variable, then the prompt. A configured
`key` is used exclusively: the agent and password are only tried when no key is
set.

#callout(kind: "note")[
  On a changed host key, the deploy stops with an error rather than trusting the
  new key. If you trust the change, run `ssh-keygen -R <host>` and retry, or set
  `strict #false`.
]

== Change detection

Baudelaire runs `sha256sum` on the host and diffs it against the local files, so
an unchanged file is never re-sent. If the host cannot run it (a bare directory,
a non-coreutils system), every file simply uploads; the deploy is still correct,
just not incremental that run.
