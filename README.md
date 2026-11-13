# notes-stats

Example app of team `notes` in the [homelab-selfuryon](https://github.com/homelab-selfuryon) org. It shows
service-to-service authentication **without shared keys**: `GET /v1/stats` calls
[`notes-api`](https://github.com/homelab-selfuryon/notes-api) with its own projected ServiceAccount token. Made
from [`rust-app-template`](https://github.com/homelab-selfuryon/rust-app-template); delivered as described
in `meta/delivery.md`.

| Method and path |                                                                                   |
| --------------- | --------------------------------------------------------------------------------- |
| `GET /healthz`  | liveness                                                                          |
| `GET /readyz`   | readiness; independent of notes-api, so this app is up when its dependency is not |
| `GET /v1/stats` | `{"source": "notes-api", "notes": {"count": n}, "env": "..."}`                    |

## How the call is authenticated

`deploy/deploy.cue` says `network: calls: [{app: "notes-api"}]`. The platform renders from that:

- an egress rule to notes-api in the CiliumNetworkPolicy,
- a projected ServiceAccount token, audience `notes-api`, mounted at
  `/var/run/secrets/tokens/notes-api/token` (the pod has no other token),
- `NOTES_API_URL=http://notes-api.<namespace>.svc.cluster.local`.

The app reads the token file **on every call**, because the kubelet rotates the token before it
expires (a token read once at start-up stops working after an hour). notes-api verifies the token against
the cluster's signing keys and checks the audience and that the subject
(`system:serviceaccount:notes-<env>:notes-stats`) is on its allowlist: there is no API key to leak, to
share or to rotate.

Failure mapping: no token file → 503; notes-api unreachable, refusing the token (401/403) or failing → 502.
The answers say nothing about internals; the log has the details. The token is never logged.

`notes-stats` is not public and (no `allowFrom`) accepts no callers: the example is about the outgoing
call. A preview environment of this app calls the **staging** notes-api, whose allowlist does not
include preview namespaces, so `/v1/stats` answers 502 there.

## Development

Tools come from devenv (no Nix flakes; `.envrc` has `use devenv`); `devenv test` is what CI runs.

```sh
devenv shell           # toolchain, cue (0.18 pre-release), cargo-deny, treefmt, git hooks
treefmt                # format everything (same config as the git hook)
cargo test             # a stand-in notes-api checks the token on every call, including rotation
devenv test            # treefmt --fail-on-change, toolchain/Containerfile match, clippy -D warnings,
                       # cargo test, cargo deny check (network: advisories)
docker buildx build -f Containerfile .   # the image (podman build works too)
devenv shell -- cue vet ./deploy
```

`devenv.lock` is not committed yet: the import needs platform-kit v0.1.0 to be published. Then
`devenv update` and commit it; refresh it the same way (Renovate has no devenv manager).
Against an unpublished platform-kit: `devenv -o platform-kit path:../platform-kit test`.

The HTTP client has no TLS stack (the call stays inside the cluster over plain HTTP, authenticated by the
token and restricted by network policy). Configuration: `NOTES_API_URL`, `NOTES_API_TOKEN_FILE`,
`UPSTREAM_TIMEOUT_SECS` (3), `LOG_LEVEL`, `SHUTDOWN_DELAY_SECS` (2), `REQUEST_TIMEOUT_SECS` (30).

Delivery (preview per pull request labelled `preview`, release-please, tag → staging, promote → prod) is the template's: see its
README. Commits follow Conventional Commits and are signed.
