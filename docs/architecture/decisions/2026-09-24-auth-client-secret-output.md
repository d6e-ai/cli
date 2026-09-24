# ADR: Explicit output for one-time Auth Client secrets

Date: 2026-09-24
Status: Accepted

## Context

d6e-auth returns `clientSecret` only when an organization-owned Auth Client is created or its secret is rotated. A failed or accidental output path can lose the only usable copy, especially after rotation.

## Decision

`d6e organization auth-client create` and `rotate-secret` require `--secret-output`. A path names a new file; `-` explicitly writes the secret response to stdout. No command prints a secret without this option. List, show, update, and revoke deserialize metadata without any secret field.

For file output on Unix, the CLI opens the target with `create_new`, sets mode `0600`, and confirms it can use the file before sending the create or rotate request. Existing files are never overwritten. If the API call fails, the reserved empty file is removed only if its filesystem identity still matches the file opened by the CLI. After the API returns a secret, the file is retained even if writing or syncing fails, since it may contain the only recoverable copy. The error is generic and contains no secret.

On platforms where owner-only file permissions are not enforced by this implementation, file output fails closed. `--secret-output -` remains available when the caller intentionally controls stdout.

The file contains `{ "clientId": "...", "clientSecret": "..." }`. Normal stdout for a file sink contains client metadata and the destination path, without `clientSecret`. Explicit stdout output contains the one-time secret. The secret response type is deserializable only and has no `Debug` or ordinary `Serialize` implementation.

## Recovery

If a create or rotate request succeeds but writing the secret fails, the server mutation may already have happened. The user should inspect the reserved file locally. If it has no complete secret, they can run `rotate-secret` again with a fresh output path. The CLI does not automatically retry a one-time secret mutation.
