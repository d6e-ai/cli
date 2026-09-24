# ADR 2026-09-24: Store CLI refresh tokens in the OS keyring

Status: Accepted

## Context

The CLI needs a session across invocations. A refresh token is longer lived than an access token and can mint more tokens. AI agents should be able to run `d6e` without reading that credential.

## Decision

Store the `d6e-cli` refresh token in the OS keyring, indexed by the configured d6e-auth origin. Keep access tokens, authorization codes, and PKCE verifiers in process memory only. Replace the stored refresh token after refresh and delete it on local logout. An invalid grant reports that login is needed without deleting the entry, since another process may have replaced it. Never fall back to a plaintext credential file or environment variable if the keyring is unavailable.

Only one CLI account is active per origin. A later login replaces that origin's credential. Command output and diagnostics must never include token values. Tests use an in-memory credential-store implementation.

## Consequences

- A locked or unavailable keyring prevents authenticated commands until the user unlocks it or logs in in a supported environment.
- Local logout removes future CLI access from that machine, but previously issued tokens may remain valid until expiry or server-side invalidation. Remote revocation needs a separately defined d6e-auth endpoint.
- The keyring is an at-rest boundary. A process running as the same user may still have access according to the operating system's keyring policy; the CLI must avoid passing credentials to agent output, arguments, or logs.
