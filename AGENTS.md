# AGENTS.md

This project's authoritative agent briefing is **[`CLAUDE.md`](CLAUDE.md)** —
role, hard invariants, session-start protocol, validation commands, gotchas,
and commit conventions all live there.

AGENTS.md is retained as a thin pointer so non-Claude harnesses (Codex, Cline,
Aider, etc.) that probe for this filename get routed to the same source of
truth. It deliberately duplicates no content; keeping the brief in one file
prevents the two from drifting (which they historically did — this file was
previously a stale fork missing Sprint 3.1 and Sprint 3.4 content).

**Before proposing or executing changes in this repository, read
[`CLAUDE.md`](CLAUDE.md).**

Per-user collaboration preferences live in `CLAUDE.local.md` (gitignored) and
are not binding on other users / harnesses.
