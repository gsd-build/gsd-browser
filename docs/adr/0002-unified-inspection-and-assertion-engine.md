# ADR 0002: Unified Inspection and Assertion Engine

- Status: Proposed
- Date: 2026-04-22

## Context

`gsd-browser` exposes multiple commands that answer the same fundamental question: what is the current browser state?

Those commands currently follow different evaluation paths:

- `eval` executes directly in the current page context
- `snapshot` builds refs from its own DOM traversal path
- `find` performs a separate query path
- `assert` evaluates a simplified compact state plus its own selector batch logic

This creates structural inconsistency:

- one command can observe live DOM state while another reports a false negative against the same page
- frame handling is not guaranteed to match across commands
- long-page, iframe, shadow DOM, or dynamic-content cases can behave differently depending on which command is used
- product semantics become command-dependent instead of state-dependent

A browser tool cannot be trustworthy when read operations disagree about the same page.

## Decision

`gsd-browser` uses one inspection engine as the source of truth for all read-style commands and for the read phase of interaction commands.

This engine defines:

- page and frame resolution
- DOM traversal
- visibility semantics
- text extraction semantics
- selector resolution
- element counting
- focused element reporting
- dialog detection
- network and console observation boundaries

`snapshot`, `find`, `page-source`, `session-summary`, `wait-for`, and `assert` consume the same engine outputs or shared primitives from that engine.

## Required Design Rules

### 1. One frame-aware query model

All read operations resolve against the same target model:

- active page
- active frame selection
- main frame fallback rules
- same-origin iframe traversal rules
- shadow DOM traversal rules

If a command cannot evaluate a target because of cross-origin boundaries, it reports that boundary explicitly.

### 2. One visibility model

The definition of visible, hidden, attached, detached, interactive, and text-visible is shared across commands.

`assert selector_visible`, `find`, and `snapshot` use the same predicate.

### 3. One text model

Text extraction semantics are shared across:

- `text_visible`
- `text_hidden`
- snapshot labels
- semantic matching
- inspection summaries

Text checks do not depend on one compact body-text snapshot while other commands inspect live nodes differently.

### 4. Assertion is a consumer, not a parallel implementation

`assert` does not own separate DOM logic. It consumes the inspection engine and log engine.

Assertion checks are built from reusable primitives:

- URL
- resolved elements
- visibility
- extracted text
- counts
- value state
- checked state
- console observations
- network observations

### 5. Observability boundaries are shared

Temporal assertions such as `no_console_errors_since` and `no_failed_requests_since` use the same action timeline and event timestamps exposed to diagnostics.

There is one definition of “since action X”.

## Non-Goals

- preserving command-specific DOM semantics
- optimizing each read command independently at the expense of consistency
- hiding cross-origin limitations behind silent false negatives

## Consequences

### Positive

- `eval`, `snapshot`, `find`, and `assert` agree on the same page state.
- False negatives drop because selector, text, and frame handling share one implementation.
- New read commands can be built from stable primitives instead of new bespoke logic.
- Debugging becomes simpler because disagreements between commands indicate a real bug, not architectural drift.

### Negative

- Refactoring cost is higher than patching individual commands.
- The inspection engine becomes a central dependency and requires stronger tests.
- Some existing output details may change as semantics become consistent.

## Implementation Requirements

1. Introduce an inspection module that returns structured frame-aware query results.
2. Move visibility, text extraction, selector evaluation, and element counting into shared primitives.
3. Make `assert` consume inspection primitives instead of direct custom JS branches.
4. Make `snapshot` and `find` reuse the same resolution rules.
5. Define explicit behavior for:
   - main frame
   - selected frame
   - same-origin iframe traversal
   - cross-origin iframe boundaries
   - open shadow roots
6. Add integration tests where all of the following commands agree on the same page:
   - `eval`
   - `snapshot`
   - `find`
   - `assert`
7. Add regression tests for:
   - iframe content
   - open shadow DOM content
   - delayed dynamic content
   - long pages with content outside the initial viewport
   - pages with dialogs and overlays

## Acceptance Criteria

- `assert` does not maintain a separate selector or text-evaluation path from the inspection engine.
- The same page state produces consistent results across `snapshot`, `find`, `eval`, and `assert`.
- A failure caused by a cross-origin frame reports that boundary explicitly.
- Visibility and text assertions match the same semantics used by element discovery.
