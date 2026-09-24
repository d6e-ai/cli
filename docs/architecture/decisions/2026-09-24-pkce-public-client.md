# ADR 2026-09-24: Use the CLI public client with PKCE

Status: Accepted

## Context

The CLI runs on a user's machine, where a bundled client secret could not remain secret. Users and AI agents need personal and organization self-service without creating a long-lived Management API Key. d6e-auth registers `d6e-cli` as a separate public OAuth client and accepts loopback redirects with PKCE S256.

## Decision

`d6e auth login` uses Authorization Code with PKCE S256, the registered `d6e-cli` client ID, and a local loopback callback. The CLI never sends a client secret. Resource commands send only the resulting `d6e-cli` audience user access token. Organization-owned confidential Auth Clients remain a separate resource managed under `d6e organization`.

## Consequences

- A person completes an interactive browser login once; AI agents invoke commands without receiving credentials.
- Callback state, exact redirect URI, a bounded listener, and a fresh verifier are required for every login attempt.
- The CLI depends on the public-client registration and its loopback policy being deployed on the selected d6e-auth origin.
- Login has no unattended bootstrap path in this design; automation uses an existing local session.
