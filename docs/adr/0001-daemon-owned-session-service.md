# ADR 0001: Daemon-Owned Session Service

- Status: Proposed
- Date: 2026-04-22

## Context

`gsd-browser` promises a persistent background daemon, named sessions, and stable stepwise browser control across separate CLI invocations.

The current architecture does not enforce that contract strongly enough:

- The CLI can complete a successful `navigate` call while a later invocation attaches to `about:blank` or an empty action history.
- A named `--session` can represent bookkeeping artifacts without guaranteeing a healthy, attached browser.
- The daemon can be alive at the PID level while unable to serve requests correctly.
- A client can only infer session health indirectly from command failures.
- Browser ownership, daemon ownership, and session identity are not modeled as a single durable unit.

This mismatch makes the most important product claim unreliable: a session name is not yet a strict identity boundary for one browser lifecycle.

## Decision

`gsd-browser` treats each session as a daemon-owned service with explicit lifecycle, durable identity, and health state.

A session is the combination of:

- session name
- daemon process
- browser process or attached CDP target
- socket endpoint
- active browser context metadata
- persisted health and recovery metadata

The CLI remains a thin RPC client. It does not infer or repair session state implicitly beyond deterministic reconnect logic defined by the daemon.

## Required Design Rules

### 1. Session identity is durable

Each named session persists a manifest on disk containing:

- session name
- daemon PID
- browser PID when launched locally
- socket path
- daemon start time
- browser start time
- daemon version
- launch mode: launched browser or attached CDP URL
- browser endpoint metadata
- health status
- last successful heartbeat time
- active page identity
- browser profile or user-data-dir location

The manifest is the source of truth for session identity and health.

### 2. A session never silently becomes a different browser

For a named session:

- a healthy manifest plus healthy daemon means reconnect
- an unhealthy manifest means explicit error with the unhealthy reason
- stale artifacts are only removed after positive proof that the daemon and browser are gone
- a new browser instance is created only through explicit recovery rules

The client does not silently fall back to a fresh blank session when a named session is unhealthy or ambiguous.

### 3. Health is explicit, not inferred

Session health has at least these states:

- `starting`
- `healthy`
- `degraded`
- `recovering`
- `stopped`
- `unhealthy`

Health checks verify:

- daemon PID existence
- socket responsiveness
- RPC round-trip success
- browser connectivity
- active target availability

`daemon health` and `session-summary` expose the same state machine.

### 4. Recovery is deterministic

Recovery logic follows a fixed order:

1. Validate manifest.
2. Verify daemon PID.
3. Verify socket connectivity.
4. Verify browser connectivity.
5. Verify active target availability.
6. Reconnect if all checks pass.
7. Return a specific unhealthy-state error if any invariant fails.
8. Start a replacement session only when the configured recovery policy explicitly allows it.

### 5. Lifecycle ownership is process-safe

The daemon starts detached from the foreground CLI process group and remains valid after the invoking shell command exits.

The daemon owns browser shutdown, socket cleanup, manifest updates, and terminal session transitions.

### 6. The session API is a product surface

The following commands expose session state directly:

- `daemon health`
- `session-summary`
- `list-pages`
- `switch-page`
- `daemon stop`

These commands report the same session identity and health information. Their output is not assembled from inconsistent sources.

## Non-Goals

- automatic silent failover to a different browser
- best-effort recovery that hides state loss
- separate “lightweight” and “full” session semantics for the same session name

## Consequences

### Positive

- Named sessions become trustworthy across separate CLI invocations.
- Session failures become diagnosable without guessing from `about:blank`.
- Multi-step agent workflows can rely on stepwise control instead of forcing one-shot chaining.
- Browser ownership becomes explicit enough to support stronger observability and recovery.

### Negative

- Session startup and recovery logic becomes more complex.
- More state must be persisted and kept consistent.
- Some current implicit recovery behavior becomes an explicit user-facing error instead.

## Implementation Requirements

The implementation includes all of the following:

1. Add a session manifest type and persistence layer in the common crate.
2. Move session health evaluation into a single reusable module.
3. Make the daemon write manifest state transitions on start, healthy-ready, degraded, recovery, and stop.
4. Make the client validate the manifest before attempting replacement behavior.
5. Make named-session replacement opt-in and explicit.
6. Make `session-summary` return manifest identity plus live health checks.
7. Add multi-process integration tests for:
   - `navigate` in one process followed by `session-summary` in another
   - named session reconnect after the original CLI process exits
   - stale PID file with dead socket
   - live daemon with broken browser connection
   - refusal to silently replace an unhealthy named session

## Acceptance Criteria

- A successful `navigate` followed by a separate `session-summary` for the same session reports the same active page URL and title.
- A named session does not return `about:blank` unless the actual active page is `about:blank`.
- A broken session reports `unhealthy` or `degraded` with a specific reason.
- A healthy named session survives the lifecycle of the command that created it.
- A named session is never replaced by a fresh browser without an explicit recovery policy.
