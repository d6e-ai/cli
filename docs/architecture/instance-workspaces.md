# Instance workspace commands

`d6e instance --instance-url ORIGIN workspace list|create` acts for the signed-in person on one selected D6E instance. `D6E_INSTANCE_URL` can supply the origin. The CLI accepts an HTTPS origin, or HTTP on `127.0.0.1` or `[::1]` for local development; userinfo, paths, query strings, and fragments are rejected. The CLI sends no bearer token to an unvalidated URL and never follows redirects.

## Request sequence

1. Refresh the existing `d6e-cli` user access token from the OS keyring.
2. POST `{ "instance_url": "<canonical origin>" }` to d6e-auth `/api/v1/auth/instances/exchange` with that CLI token. The canonical origin has no trailing slash, for example `https://instance.example.com`. D6E Auth identifies an active registered instance from the origin and returns `{ "access_token", "token_type": "Bearer", "expires_in", "instance_url" }` for its audience. The CLI requires the echoed origin to exactly match the requested canonical origin.
3. Send the exchanged token to the selected instance frontend's `/api/cli/workspaces` endpoint: `GET` for `list`, `POST { "name": "..." }` for `create`. The token remains only in memory for this invocation. The instance must validate its audience and resolve current workspace membership itself.

The instance frontend returns `{ "workspaces": [...] }` for list. Create returns `{ "workspace": {...}, "billing": { "status": "pending" | "provisioning_failed", "warning"?: "..." } }`. Both are wrapped by the CLI's normal JSON `data`/`meta` envelope. A workspace can exist even if pending subscription provisioning failed; the CLI displays that status instead of reporting full readiness. A retry of `create` can create another workspace, so users should inspect `list` before retrying after a transport error.

The CLI does not store or print the exchanged access token. It does not handle instance service credentials or billing directly. Workflow execution and chat are separate future commands.
