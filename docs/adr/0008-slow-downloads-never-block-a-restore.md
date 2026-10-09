# ADR-0008: Slow downloads never block a restore

- **Status:** Accepted
- **Date:** 2026-10-09
- **Decided by:** Sumiran

## Context
A real 193 MB JDK download crawled at 10 kB/s (≈5 h) and held the whole restore.

## Decision
Every external install runs under a generic watchdog; crawling or stalled installs are deferred, never failed; rerunning restore finishes them (docs/08).

## Consequences
Per-item installs; a deferred state in reports; parity tests for the Rust port.
