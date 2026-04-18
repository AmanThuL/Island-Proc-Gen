---
id: braun_2023_implicit_spim
title: "Implicit finite-difference schemes for SPIM discretization on adaptive meshes"
authors: Braun, Seixas, Lebedev
year: 2023
venue: Computers & Geosciences
doi: 10.1016/j.cageo.2023.105128
pdf_status: metadata_only
pack: sprint_2_deferred
sprint_first_used: sprint_4
---

# Braun et al. 2023 — Implicit SPIM Solver

**Status:** metadata-only. Sprint 2 defers implicit solver to Sprint 4 GPU compute sprint. See `docs/design/sprints/sprint_2_geomorph_credibility.md` §11 "为什么 Sprint 2 不上 implicit" and §7 "不做" sections. This file exists as a forward-reference so the "已调研 vs 未读" status is tracked.

## Why deferred

Explicit Euler 4-substep hillslope diffusion + 10 outer iterations under v1 `K = 1e-3` is numerically stable. Implicit tridiagonal solver introduces significant implementation complexity without justification at 256² resolution. Single-threaded outer-loop runtime remains well below 200ms target (per Sprint 1C headless baseline). Braun 2023's implicit scheme is deferred until GPU productization enables cost amortization.

## Trigger for upgrade

- Sprint 4 GPU productization (compute budget allows implicit solver + tridiagonal decomposition overhead)
- If Sprint 3 sediment transport forces adoption of non-linear `n != 1`, implicit becomes mandatory to maintain CFL stability
- If higher resolution (512² or 1024²) pushes per-iteration cost beyond 50ms, revisit Braun 2023 for v2.1 optimization

## Cross-references

- Sprint 2 §RD3 (explicit stability analysis; links to why `n=1.0` + forward-Euler is sufficient v1)
- Kwang & Parker 2017 (establishes that `m/n = 0.35` avoids pathology even with explicit integration)
- Whipple & Tucker 1999 (stability theory for non-linear SPIM; implicit becomes necessary if `n > 1`)
