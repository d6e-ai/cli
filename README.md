# D6E CLI

`d6e` is a Rust CLI for managing your D6E account. It is designed for AI agents to run commands using your local sign-in session without receiving OAuth tokens.

```sh
cargo install --path .
d6e auth login
d6e personal show
d6e personal update --name "Your name"
d6e organization list
d6e organization create --name "Example Ltd."
d6e organization profile update ORGANIZATION_ID --legal-name "Example Ltd." --country SG
d6e organization auth-client create ORGANIZATION_ID --name "App" --secret-output ./app-secret.json
d6e instance --instance-url https://instance.example.com workspace list
d6e instance --instance-url https://instance.example.com workspace create --name "My workspace"
d6e auth logout
```

Use `d6e auth login --no-open` when you need to open the sign-in URL manually. `D6E_AUTH_URL` defaults to `https://www.d6e.ai`; for local development, use an HTTP URL on `127.0.0.1` or `[::1]`.

`d6e instance workspace` needs an instance origin from `--instance-url` or `D6E_INSTANCE_URL`. Sign in first. Workspace creation reports billing provisioning status in its JSON result; a created workspace may still need billing setup.

Commands return one JSON object on stdout. Errors return one JSON object on stderr with a stable code and exit status. The CLI stores only the refresh token in the OS keyring. `auth logout` removes this machine's credential; it does not revoke already issued tokens on the server.

Architecture and API details are in [docs/architecture](docs/architecture/overview.md).

Organization commands include `list`, `show`, `create`, `update`, and `profile show|update`. Profile updates accept `--legal-name`, `--country`, `--billing-email`, `--phone`, and `--address-line1`, `--address-line2`, `--address-city`, `--address-state`, `--address-postal-code`. Use the matching `--clear-*` flag to remove a field, or `--clear-address` to remove the entire address. The CLI reads the latest profile ETag before updating and reports a conflict if another update wins the race.

`organization auth-client` manages application credentials. Creating or rotating a secret requires `--secret-output` with a new file path; pass `-` only when you explicitly want the secret on stdout. File output is supported on Unix.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```
