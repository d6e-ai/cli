# d6e-auth API contract

The canonical user API contract is [`d6e-auth/docs/architecture/contracts/d6e-cli-user-api.v1.json`](https://github.com/d6e-ai/d6e-auth/blob/a2b0f31950a1377a8231ba069b585851e2d81419/docs/architecture/contracts/d6e-cli-user-api.v1.json), inspected at d6e-auth commit `a2b0f31950a1377a8231ba069b585851e2d81419`. This repository keeps a [pinned copy](contracts/d6e-cli-user-api.v1.json) for tests. Review upstream changes before replacing it.

## Authentication and errors

- Resource base path: `/api/v1`; JSON request/response bodies.
- Resource requests carry `Authorization: Bearer <access token>`. The access token has one audience, `d6e-cli`, and identifies the user through `sub`. Refresh tokens and multiple-audience tokens are rejected by these routes.
- d6e-auth resolves live membership and role for each organization request. Inaccessible or foreign organization resources return `404`.
- Errors contain string `error` and `message` fields. Preserve `X-Request-Id` when reporting a failed request. Responses are `private, no-store`.

The OAuth token endpoint `/api/v1/auth/token` is specified by d6e-auth's route code rather than the resource contract JSON. It accepts JSON or form-encoded string fields. Both authorization-code and refresh-token grants return `access_token`, `refresh_token`, `token_type=Bearer`, and `expires_in`; currently `expires_in` is 3600 seconds. A public-client request must omit `client_secret`.

## First slice: personal profile

| Command | HTTP operation | Contract |
| --- | --- | --- |
| `d6e personal show` | `GET /api/v1/me` | Returns `user.id`, `email`, `name`, `updatedAt`. |
| `d6e personal update --name NAME` | `PATCH /api/v1/me` | Sends only `{ "name": "..." }`; server trims it and requires 1–100 characters. |

## Organization operations

`d6e organization list|show|create|update` use the corresponding organization endpoints. Name changes require owner or admin. Organization identifiers are UUIDs, validated before constructing a route path.

`d6e organization profile show|update` use profile GET/PATCH. Update first reads the current profile and sends its `ETag` as `If-Match`; omitted fields are preserved, explicit `null` clears a field, and a stale version returns `409` without an automatic retry. An absent profile has version `0`. The CLI treats a missing GET `ETag` as a protocol error. Server `428` responses are surfaced with their API code and request ID.

`d6e organization auth-client list|show|create|update|revoke|rotate-secret` uses the v1 Auth Client operations. The canonical rotation endpoint is `POST /api/v1/organizations/{organizationId}/auth-clients/{clientId}/secret` with `{}`. `revoke` sends exactly `{ "status": "inactive" }` through PATCH; physical deletion is not exposed. An Auth Client may be addressed by UUID or its generated `d6e_` client ID.

The organization Auth Client create and rotate-secret responses reveal `clientSecret` exactly once. List, get, update, and revoke responses contain no secret. Creation and rotation require an explicit secret destination, prepared before the mutation. The secret is excluded from ordinary JSON output, errors, and logs. See the [one-time secret output decision](decisions/2026-09-24-auth-client-secret-output.md).

The contract excludes organization deletion/status changes, membership/invitation management, tax ID mutation, and Auth Client deletion. A CLI operation must not infer authority from JWT display claims or from a cached membership list.
