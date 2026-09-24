# D6E CLI architecture

`d6e` is a Rust CLI for a signed-in person to manage their D6E profile and organizations. It is designed for AI agents to invoke ordinary commands without receiving OAuth tokens. The CLI calls d6e-auth directly; it does not add a plugin host or a separate management API key.

## Boundaries

- `d6e auth` owns interactive login, local credentials, token refresh, status, and local logout.
- `d6e personal` reads the current user and changes that user's name through `/api/v1/me`. No user ID is accepted from the command line.
- `d6e organization` manages organizations the current user may access. The server checks current membership and role on every request; CLI-side checks are only for usability.
- Organization-owned Auth Clients and their one-time client secrets belong under `d6e organization`. An Auth Client is a confidential client for an application, not the CLI's own public client.
- Instance/workspace and skill commands are later phases. They do not change this authentication boundary.

The first end-to-end slice is `auth login|status|logout` plus `personal show|update`. Organization and Auth Client operations follow after that path is verified against d6e-auth.

## Components

1. The command parser validates options and selects an operation.
2. The authentication component obtains an access token from a local login or refreshes it. Its secret store is an OS keyring adapter.
3. The API client sends the access token as `Authorization: Bearer` to d6e-auth, parses the documented response, and preserves `X-Request-Id` for diagnostics.
4. Command handlers map API objects to a stable JSON result or structured error. One-time Auth Client secrets use an explicit output destination.

See [authentication.md](authentication.md) for the login sequence and [api-contract.md](api-contract.md) for the resource contract. The decisions are recorded in [PKCE public client](decisions/2026-09-24-pkce-public-client.md) and [local credential storage](decisions/2026-09-24-local-credential-storage.md).
