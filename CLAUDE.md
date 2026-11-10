# notes-stats

Calls notes-api with a projected ServiceAccount token; team `notes`. A copy of `rust-app-template`
without its token-verification module (nothing calls this app). Contract: `meta/delivery.md`, card
`meta/notes-stats.md`.

## Layout

```
src/notes_api.rs  the client: token re-read from disk on every call, errors mapped without internals
src/routes.rs     /v1/stats, health; src/config.rs env config
tests/stats.rs    a stand-in notes-api that accepts only the current token
deploy/deploy.cue not public; network.calls notes-api
devenv.yaml/.nix  tools (platform-kit devenv module), Containerfile image build
```

## Commands

```sh
devenv shell -- cargo test                         # no database needed
devenv shell -- treefmt                            # format (devenv test uses --fail-on-change)
devenv test                                        # the CI contract: treefmt, toolchain/Containerfile match,
                                                   # clippy -D warnings, cargo test, cargo deny check
devenv shell -- cue vet ./deploy
docker buildx build -f Containerfile .             # or: podman build -f Containerfile .
devenv shell -- actionlint && devenv shell -- zizmor --offline .github/workflows
```

devenv.lock is not committed until platform-kit v0.1.0 is published (the import needs it); then
`devenv update` and commit; refresh it the same way. Against an unpublished platform-kit:
`devenv -o platform-kit path:../platform-kit test`.

## Rules

- The token file is read on every call; never cache the token, never log it.
- The platform provides `NOTES_API_URL` and the token mount from `network.calls`; keep the defaults in
  `config.rs` in line with `meta/delivery.md` ("Mount points").
- reqwest without a TLS feature on purpose: in-cluster plain HTTP only.
- Conventional Commits, `!` for breaking changes, signed commits.
