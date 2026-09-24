# D6E CLI

`d6e` is a Rust CLI for managing your D6E account. It is designed for AI agents to run commands using your local sign-in session without receiving OAuth tokens.

```sh
cargo install --path .
d6e auth login
d6e personal show
d6e personal update --name "Your name"
d6e auth logout
```

Use `d6e auth login --no-open` when you need to open the sign-in URL manually. `D6E_AUTH_URL` defaults to `https://www.d6e.ai`; for local development, use an HTTP URL on `127.0.0.1` or `[::1]`.

Commands return one JSON object on stdout. Errors return one JSON object on stderr with a stable code and exit status. The CLI stores only the refresh token in the OS keyring. `auth logout` removes this machine's credential; it does not revoke already issued tokens on the server.

Architecture and API details are in [docs/architecture](docs/architecture/overview.md).

## Development

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```
