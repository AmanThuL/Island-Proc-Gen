# CLAUDE.md

Context for Claude Code (and any other agent harness) working in this
repository. Read this before proposing or executing changes.

---

## Role

Pair programmer on a single-developer Rust research project. Default stance:
help me build, catch mistakes, push back when an idea drifts from the active
sprint's stated scope. Prefer small atomic commits over big bundled ones. Ask
before anything irreversible — force push, dep downgrades, renaming a workspace
crate, rewriting `WorldState` layout, deleting generated artifacts.

---

## Key files

| File | Purpose |
|------|---------|
| [`PROGRESS.md`](PROGRESS.md) | Sprint-level dashboard — what's shipped, what's next, what's blocked |
| [`docs/design/island_generation_complete_roadmap.md`](docs/design/island_generation_complete_roadmap.md) | Authoritative roadmap and architectural rules |
| [`docs/design/sprints/sprint_N_*.md`](docs/design/sprints/) | The active sprint's implementation plan and §6 acceptance checklist |
| [`docs/papers/README.md`](docs/papers/README.md) | Paper knowledge base layering (A Core Pack / B Sprint Packs / C Case Studies / D Parking Lot) |

**Read the active sprint doc before touching code for that sprint.** The
sprint's §6 acceptance checklist and §7 risks/invariants are the done-definition
— not generic Rust best practices.

---

## Architectural invariants (hard rules — do not weaken without flagging)

These are enforced by tests and CI, not just convention. Breaking any of them
reverts to `dev` and re-opens the sprint.

1. **`core` stays headless.** `cargo tree -p core` must never list `wgpu`,
   `winit`, `egui*`, `png`, `image`, or `tempfile`. The
   `pipeline_runs_without_graphics` test in `crates/core/src/pipeline.rs`
   enforces this at the test level.
2. **No `&Path` or `std::fs` in `core`.** The save codec is byte-level
   (`impl Write` / `impl Read`); `app::save_io` is the only ~5-line Path
   wrapper. Wasm target must work without touching `core`.
3. **`WorldState` is three-layer.** Top-level fields are exactly
   `{ seed, preset, resolution, authoritative, baked, derived }`. Never add
   `Option<ScalarField2D<...>>` to the top level — put it under `authoritative`
   / `baked` / `derived`. `derived` is `#[serde(skip)]`.
4. **`Resolution` is simulation-only.** `sim_width` / `sim_height` live on
   `WorldState`. Render LOD and hex columns/rows live in their own crates and
   are NOT part of canonical state.
5. **No `Vec<bool>`.** Masks are `MaskField2D = ScalarField2D<u8>` with the
   `0 = false / 1 = true` convention, so GPU upload / PNG export / serde are
   contiguous byte arrays.
6. **Field abstraction is not a trait.** `ScalarField2D<T>` + `MaskField2D` +
   `VectorField2D` aliases only. If you catch yourself writing `trait Field`,
   stop. (The `pub(crate) trait FieldDtype` used internally to seal
   `to_bytes` / `from_bytes` over `u8|u32|f32|[f32; 2]` is OK — it's private.)
7. **Overlays are descriptors, not closures.** `OverlayRegistry` stores
   `Vec<OverlayDescriptor>`. Any "render closure" pattern locks Sprint 4's
   CPU-side PNG export path and must be rejected.
8. **String field keys only in `crates/render/src/overlay.rs`.** `crates/sim`,
   `crates/core::save` (error-message payloads aside), and
   `crates/core::validation` access state via struct field paths like
   `world.authoritative.height` — not by stringly-typed dispatch.

---

## Crate dependency direction

```
app ──▶ render ──▶ gpu ──┐
  │       │              │
  │       └──▶ core ◀────┘
  │              ▲
  ├──▶ ui ───────┘
  │              ▲
  └──▶ sim ──────┘
         ▲
  hex ───┘
  data ──▶ core
```

`core` is a sink. `app` is the only crate allowed to wire everything together.

---

## Gotchas (learned, not in the roadmap)

- **The `core` crate name shadows stdlib `::core`.** Downstream crates (`app`,
  `data`, `ui`) import it as
  `island_core = { path = "../core", package = "core" }`. Apply the same
  pattern when adding a new crate that depends on `core`.
- **`crates/core/Cargo.toml` has `[lib] doctest = false`.** Same shadowing
  issue: rustdoc runs `--crate-name core`, and `thiserror`'s derive expands
  `::core::fmt` paths that can't resolve inside the user crate. Don't remove
  it until a future sprint decides whether to rename `core` → `ipg-core`.
- **`ScalarField2D<T>` field payloads are NOT serde-serialized inside
  `WorldState`.** `authoritative.height` and `authoritative.sediment` are
  individually `#[serde(skip)]`; the save codec writes them via
  `ScalarField2D::to_bytes()` rather than serde, to keep the `IPGF` byte format
  under our control and avoid double-encoding.
- **Bash invocations don't auto-source cargo env on this machine.** Prefix
  commands with `. "$HOME/.cargo/env" && <command>`. `~/.bashrc` has the
  source line, but non-interactive bash skips it.
- **Version pins (locked in Sprint 0).** `egui` / `egui-wgpu` / `egui-winit` at
  `0.34.1`; `wgpu` `29.0.1`; `winit` `0.30.13`. Winit 0.30 uses the
  `ApplicationHandler` trait pattern, not the legacy `EventLoop::run` closure.
  Don't mix versions without verifying the egui / wgpu compatibility matrix.

---

## Commit style

- **Conventional commits:** `feat(scope): ...`, `fix(scope): ...`, `refactor: ...`,
  `docs: ...`, `ci: ...`, `chore: ...`. Scope is crate name(s) for code changes
  (`feat(core,app): ...`) or omitted for workspace-wide refactors.
- **One task per commit.** Sprint-level work is bundled across multiple commits,
  not one giant commit — makes bisection and rollback tractable.
- **No `Co-Authored-By: Claude ...` footer** — attribution is disabled globally
  at the user level.
- Don't amend commits that are already on `dev` or `main`. Create a new commit.
- Don't bypass `--no-verify` or `--no-gpg-sign` without asking.

---

## Rules for this session

1. The active sprint doc's §6 acceptance checklist is the done-definition.
   Features beyond it are out of scope unless I explicitly ask.
2. Before running `cargo run -p app` (which opens a window) or any `git push`,
   check with me first.
3. Use subagents for substantial implementation work so my main context stays
   clean. Match model to task complexity:
   - **Haiku** for mechanical scaffolding (config files, CI yaml, renames)
   - **Sonnet** for typical implementation (new modules, feature wiring,
     research tasks)
   - **Opus** for architecturally load-bearing tasks (e.g. `WorldState` layout,
     save-codec invariants, the main event loop)
4. Never add a dep to `core` that breaks `cargo tree -p core` cleanliness
   (no `wgpu`, `winit`, `egui*`, `png`, `image`, `tempfile` — ever).
5. If a subagent's plan would violate any architectural invariant above, stop
   and flag it — don't let it slide.

---

## Session start protocol

1. Read `PROGRESS.md` for current sprint state.
2. Read the active sprint file in `docs/design/sprints/` for acceptance criteria.
3. Run `git status` and `git log --oneline -10` to see where the branch is.
4. If a Rust change is planned, verify `cargo check --workspace` is green
   before starting:
   ```bash
   . "$HOME/.cargo/env" && cargo check --workspace
   ```
5. Surface any architectural invariant the planned work would touch, and
   confirm the plan preserves it.

---

## Notes

- The app is macOS-first (Metal backend), but the architecture stays
  platform-agnostic — Sprint 5 targets wasm. Don't pull in
  `#[cfg(target_os = "macos")]` in `core`, ever.
- The paper knowledge base under `docs/papers/` is intentionally shallow for
  most papers (frontmatter + abstract + one-sentence purpose). Chen 2014 and
  Temme 2017 are the only ones with substantive 落地点 sections as of Sprint 0.
  Sprint 1A will fill more during its first-read phase.
