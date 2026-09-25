# ADR: Exchange CLI tokens for a selected D6E instance

Date: 2026-09-26

Status: accepted for the first instance workspace slice

## Context

The CLI's public PKCE client receives user tokens whose audience is `d6e-cli`. A D6E instance checks its own client ID as audience. Workspace creation also needs the instance frontend's billing provisioning sequence, not just the Rust workspace API call.

## Decision

The user selects an instance by HTTPS origin (loopback HTTP during local development). The CLI presents its current user token to d6e-auth for a short-lived instance-audience token. D6E Auth resolves the origin against active registered instances and echoes the canonical origin. The CLI checks that echo and calls the instance frontend's dedicated CLI workspace endpoint. Instance-side membership checks and billing provisioning are authoritative.

No instance service secret is distributed to the CLI. The exchanged token is kept in process memory and never written to the keyring or output. HTTP redirects are not followed with bearer credentials.

## Consequences

This first slice requires coordinated d6e-auth and instance frontend endpoints. Instance discovery is deferred; the user or agent must provide the intended origin. Workspace creation may succeed before billing provisioning does, so the response includes a separate billing status.
