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

## Subsequent organization operations

The v1 contract includes organization list/create/get/update; organization profile get/patch; and organization Auth Client list/get/create/update/revoke/rotate-secret. Organization name changes require owner or admin. Profile patch requires the `ETag` returned by profile get as `If-Match`; omitted fields are preserved, explicit `null` clears a field, and a stale version returns `409`. An absent profile has version `0`.

The organization Auth Client create and rotate-secret responses reveal `clientSecret` exactly once. List, get, update, and revoke responses contain no secret. The CLI must require an explicit secret destination and must not put the secret into logs, ordinary JSON output, errors, or fixtures.

The contract excludes organization deletion/status changes, membership/invitation management, tax ID mutation, and Auth Client deletion. A CLI operation must not infer authority from JWT display claims or from a cached membership list.
