# AGENTS.md

## gsd-browser

Native Rust browser automation CLI for AI agents. Controls Chrome/Chromium via CDP with a persistent background daemon. Supports navigation, element interaction (click, type, fill), snapshots with versioned refs, assertions, form analysis, structured data extraction, visual regression testing, network mocking, device emulation, auth vault, and prompt injection scanning.

### Quick Start

```bash
gsd-browser navigate https://example.com
gsd-browser snapshot
gsd-browser click-ref @v1:e3
```

### Installation

```bash
npm install -g @gsd-build/gsd-browser
```

Requires Chrome or Chromium installed on the system.

### Full Reference

See [SKILL.md](./SKILL.md) for complete command reference (63 commands), workflow patterns, configuration, and examples.
