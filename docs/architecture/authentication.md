# CLI authentication

The CLI uses d6e-auth's registered public client `d6e-cli`. It has no client secret. Login uses Authorization Code with PKCE S256 and a loopback callback; API calls use a user access token whose sole audience is `d6e-cli`. The default production origin is `https://www.d6e.ai`, whose `/api/v1/me` currently serves the v1 JSON API.

## Login sequence

1. Bind a listener to `127.0.0.1` on an available port before opening the browser. The redirect URI is exactly `http://127.0.0.1:<port>/callback`. d6e-auth also registers `[::1]`, but the initial CLI implementation may use IPv4 only.
2. Generate an unpredictable `state` and a 43–128 character PKCE verifier. Derive the base64url SHA-256 challenge without padding.
3. Open `/auth/login` with `client_id=d6e-cli`, the exact redirect URI, `state`, `code_challenge`, and `code_challenge_method=S256`. `--no-open` may present the URL for manual opening.
4. Accept a callback on the bound address and `/callback` path. Ignore unrelated or invalid requests, validate `state`, reject an OAuth error or missing code, and time out if the browser never returns. Do not reuse a code or verifier after a failed attempt.
5. POST `grant_type=authorization_code`, `client_id=d6e-cli`, `code`, the same `redirect_uri`, and `code_verifier` to `/api/v1/auth/token`. Do not send `client_secret`.
6. Store the returned refresh token in the OS keyring. Use the access token for `/api/v1/me` and resource calls. Never print either token in normal command output or errors.

The server accepts loopback HTTP only for registered host/path combinations with an explicit port from 1024 to 65535. It rejects userinfo, query, fragment, a different path, and a different host. Authorization codes expire after five minutes and are single-use. The CLI should use a shorter bounded callback wait and keep the verifier in memory only for that attempt.

## Subsequent commands

Read the refresh token from the keyring when a fresh access token is needed. Exchange it at `/api/v1/auth/token` with `grant_type=refresh_token`, `client_id=d6e-cli`, and `refresh_token`, without a client secret. Replace the stored refresh token when the server returns a new one. Keep access tokens in process memory for the current invocation. On `invalid_grant`, remove the unusable local credential and ask the user to log in again.

`auth status` may query `/api/v1/me` to identify the signed-in user; it must not disclose tokens. `auth logout` deletes the CLI's local credential. Server-side token revocation is not part of the currently inspected token endpoint, so logout must not claim to invalidate already issued tokens remotely.

If the keyring is locked or unavailable, report a clear authentication error. Do not silently write tokens to a plaintext file or expose them through environment variables. The keyring entry is scoped to the configured d6e-auth origin and one active account per origin; logging in again replaces that origin's credential.
