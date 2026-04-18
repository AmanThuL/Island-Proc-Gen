---
id: yuan_2019_efficient_spim
title: "Efficient stream power incision models for high-resolution DEMs"
authors: Yuan, Attal, Duclaux
year: 2019
venue: Earth Surface Dynamics
doi: 10.5194/esurf-7-1087-2019
pdf_status: metadata_only
pack: sprint_2_deferred
sprint_first_used: sprint_4
---

# Yuan et al. 2019 — Efficient SPIM

**Status:** metadata-only. Sprint 2 defers high-order RK integration schemes to Sprint 4 GPU pivot. See `docs/design/sprints/sprint_2_geomorph_credibility.md` §11 "为什么 Sprint 2 不上... 高效 SPIM" and §7 "不做" sections. This file exists as a forward-reference so the "已调研 vs 未读" status is tracked.

## Why deferred

Sprint 2's explicit forward-Euler + 4-substep diffusion scheme achieves target throughput (outer-loop < 200ms at 256²) without higher-order RK complexity. Yuan 2019's efficient SPIM (RK3, RK4, SSPRK methods) trades per-timestep accuracy for larger stable `dt`, but Sprint 2's v1 design prioritizes minimal dependencies and determinism. The gain in `dt` per iteration is not worth the added code surface in Sprint 2's single-threaded CPU context.

## Trigger for upgrade

- Sprint 4 GPU productization (compute budget allows RK4 + multiple-stage kernel dispatch)
- If resolution scales to 512² or 1024² and outer-loop target shrinks below 50ms, revisit Yuan 2019 for improved CFL utilization
- If sediment-transport coupling introduces stiff PDE pairs, RK3/RK4 become necessary for accuracy without unreasonable `dt`

## Cross-references

- Sprint 2 §RD3 (explicit timestep analysis; justifies why simple Euler + iteration count is sufficient for v1)
- Lague 2014 (field-data grounding for acceptable error tolerance in SPIM approximations)
- Braun 2023 (implicit alternative to Yuan's explicit high-order schemes; both deferred together to Sprint 4)
