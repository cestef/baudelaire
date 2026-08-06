#let frontmatter = (
  title: "SSH & SFTP",
  order: 10,
)
#import "/templates/theme.typ": callout

`baudelaire deploy` mirrors the built site into a directory on any host you can
reach over SSH, transferring files with SFTP:

```kdl
deploy {
  ssh {
    host "example.com"
    path "/var/www/site"
    key "~/.ssh/id_ed25519"
  }
}
```

```sh
baudelaire deploy --dry-run
```

Missing parent directories under `path` are created as files land in them, so
the remote tree matches `public/` without any setup beyond the root existing.

#callout(kind: "warn")[
  This destination needs the `ssh` cargo feature, which the `full` release has
  and the `slim` one drops. On a slim binary the block is skipped with a
  warning, or errors when it is the only destination. See
  #link("../../start/install.typ")[Install].
]

== Keys

#table(
  columns: 4,
  align: (left, left, left, left),
  table.header([Key], [Type], [Default], [Does]),
  [`host`], [str], [--], [Hostname or IP of the server. Required.],
  [`path`],
  [str],
  [--],
  [Absolute remote directory the build is mirrored into. Required.],

  [`port`], [int], [`22`], [Port the SSH server listens on.],
  [`user`], [str], [`$USER`], [User to authenticate as.],
  [`key`],
  [path],
  [--],
  [Private key to authenticate with. Unset tries the agent, then a password.],

  [`strict`],
  [flag],
  [`#true`],
  [Verify the server's key against `known_hosts`.],

  [`delete`],
  [flag],
  [`#true`],
  [Remove remote files under `path` that the build no longer produces.],
)

`key` is absolute, `~`-relative, or relative to the project root. Prefer an
ed25519 key.

An unset `user` falls back to `$USER`, and a deploy where that is unset or empty
fails rather than guessing an account. Containers, CI jobs and systemd units
routinely have no `$USER`, so state it when the deploy runs unattended:

```kdl
deploy {
  ssh {
    host "example.com"
    path "/var/www/site"
    user "deploy"
    port 2222
  }
}
```

== Authentication

Three methods, tried in order:

+ The configured `key`. Supply its passphrase, if it has one, the same way as a
  password below.
+ The *ssh-agent* at `$SSH_AUTH_SOCK`, offered every identity it holds.
+ A *password*.

A configured `key` is used exclusively. The agent and password are only reached
when no `key` is set.

The password (or key passphrase) is resolved in this order:

#table(
  columns: 2,
  align: (left, left),
  table.header([Source], [When to use it]),
  [`--secret -` (stdin)], [Piping a secret without exposing it in `argv`.],
  [`BAUDELAIRE_SSH_PASSWORD`], [CI, where a variable is already the mechanism.],
  [An interactive prompt], [A terminal, with neither of the above set.],
)

```sh
echo "$PASSPHRASE" | baudelaire deploy --secret - --yes
```

== Host keys

`strict` is the man-in-the-middle guard, and it mirrors OpenSSH. A key already
in `~/.ssh/known_hosts` is trusted, an unseen host is learned on first connect,
and a *changed* key is refused before anything uploads.

#callout(kind: "note")[
  If you trust the change (a rebuilt server, a rotated key), run
  `ssh-keygen -R <host>` and retry. On any port but 22 the entry is written
  `[host]:port`, so removing it takes `ssh-keygen -R '[host]:2222'`, quoted
  because the brackets are a shell glob. The diagnostic prints the exact
  command for the port it checked.
]

`strict #false` accepts any key, the equivalent of
`StrictHostKeyChecking=no`. The key is still checked, and a changed one warns
rather than passing in silence.

== Change detection

Baudelaire runs `find . -type f -exec sha256sum {} +` in `path` and diffs the
result against the local files, so an unchanged file is never re-sent. Nothing
is stored locally, which means a fresh checkout re-deploys nothing.

If the host cannot answer (a missing directory, a system without coreutils),
every file reads as new and the whole site uploads. The deploy is still correct,
only not incremental that run.
