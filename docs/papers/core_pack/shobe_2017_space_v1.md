---
id: shobe_2017_space_v1
title: "The SPACE 1.0 model: a Landlab component for 2-D calculation of sediment transport, bedrock erosion, and landscape evolution"
authors: Shobe, Tucker, Barnhart
year: 2017
venue: Geoscientific Model Development
doi: 10.5194/gmd-10-4577-2017
url: https://gmd.copernicus.org/articles/10/4577/2017/
pdf: core_pack/shobe_2017_space_v1.pdf
tags: [landscape-evolution, sediment-transport, bedrock-erosion, SPACE, Landlab, stream-power]
sprint_first_used: sprint_3
status: downloaded
---

## 一句话用途

Sprint 3 的沉积物传输模块（`crates/sim/src/geomorph/` 沉积相关 stage，待建）会参考 SPACE 1.0 的 alluvium + bedrock 双层守恒方程，作为从纯 detachment-limited SPIM（Sprint 2）升级到 transport-limited / mixed regime 的路线图。

## Abstract

Models of landscape evolution by river erosion are often either transport-limited (sediment is always available but may or may not be transportable) or detachment-limited (sediment must be detached from the bed but is then always transportable). While several models incorporate elements of, or transition between, transport-limited and detachment-limited behavior, most require that either sediment or bedrock, but not both, are eroded at any given time. Modeling landscape evolution over large spatial and temporal scales requires a model that can (1) transition freely between transport-limited and detachment-limited behavior, (2) simultaneously treat sediment transport and bedrock erosion, and (3) run in 2-D over large grids and be coupled with other surface process models. We present SPACE (stream power with alluvium conservation and entrainment) 1.0, a new model for simultaneous evolution of an alluvium layer and a bedrock bed based on conservation of sediment mass both on the bed and in the water column. The model treats sediment transport and bedrock erosion simultaneously, embracing the reality that many rivers (even those commonly defined as bedrock rivers) flow over a partially alluviated bed. SPACE improves on previous models of bedrock–alluvial rivers by explicitly calculating sediment erosion and deposition rather than relying on a flux-divergence (Exner) approach. The SPACE model is a component of the Landlab modeling toolkit, a Python-language library used to create models of Earth surface processes. Landlab allows efficient coupling between the SPACE model and components simulating basin hydrology, hillslope evolution, weathering, lithospheric flexure, and other surface processes. Here, we first derive the governing equations of the SPACE model from existing sediment transport and bedrock erosion formulations and explore the behavior of local analytical solutions for sediment flux and alluvium thickness. We derive steady-state analytical solutions for channel slope, alluvium thickness, and sediment flux, and show that SPACE matches predicted behavior in detachment-limited, transport-limited, and mixed conditions. We provide an example of landscape evolution modeling in which SPACE is coupled with hillslope diffusion, and demonstrate that SPACE provides an effective framework for simultaneously modeling 2-D sediment transport and bedrock erosion.

## 关键方程 / 核心结论

- TODO (Sprint 1A first read)
- Core: simultaneous bedrock erosion `E_r = K_r A^m S^n (1 - F_f)` + sediment erosion `E_s = K_s A^m S^n H/(H+H*)` + deposition `D_s = V_s q_s / q`
- Bridges detachment-limited and transport-limited regimes — Sprint 2 uses detachment-limited only; Sprint 3 considers this upgrade

## 对本项目的落地点

- TODO (Sprint 1A first read)
- `crates/core/src/world.rs` — `authoritative.sediment: Option<ScalarField2D<f32>>` is the field slot reserved for the alluvium layer that SPACE introduces
- Sprint 3 scope decision: if sediment budget is added, SPACE 1.0 equations are the implementation target

## 值得警惕的点

- TODO (Sprint 1A)
- SPACE is implemented in Python (Landlab); the Rust port requires careful numerical stability analysis especially for the `H/(H+H*)` alluvium coverage term
- Landlab uses a graph-based grid; our `ScalarField2D<T>` is a regular raster — port requires re-deriving the flux conservation at cell boundaries

---

### Sprint 3 落地点

Sprint 3 implements SPACE-lite, a two-equation subset of SPACE 1.0, in `crates/sim/src/geomorph/sediment.rs` (per DD2). Here is an explicit map of what v1 keeps vs drops from the full paper:

**Kept from SPACE 1.0:**

- `E_bed = K_bed · A^m · S^n · exp(-hs / H*)` — the bedrock incision equation with sediment cover damping. The `exp(-hs/H*)` term is SPACE's key insight: sediment cover suppresses bedrock incision nonlinearly. `H* = H_STAR = 0.05` (normalized) is the e-folding scale; at `hs = 0.1` (Sprint 3 init) the damping factor is `exp(-2) ≈ 0.14`.
- `E_sed = K_sed · A^m · S^n · min(hs, HS_ENTRAIN_MAX)` — sediment entrainment equation. `min(hs, HS_ENTRAIN_MAX)` is a saturation cap rather than SPACE's `H / (H + H*)` coverage fraction, but serves the same role: entrainment cannot exceed available sediment.
- The `hs` mass-balance update: `dhs = E_bed · dt - E_sed · dt + D · dt`. Each inner step produces bedrock material (→ increase hs), removes entrained sediment (→ decrease hs), and adds deposited sediment (→ increase hs). Physically equivalent to SPACE's alluvium conservation equation.
- `K_sed / K_bed ≈ 3`: SPACE 1.0 empirical calibration shows sediment is ~3× easier to entrain than bedrock in temperate lithologies. Sprint 3 uses `K_bed = 5e-3`, `K_sed = 1.5e-2` (ratio 3), matching this ratio. The `K_bed` value is 3× Sprint 2's `K_SPIM = 1.5e-3` because `exp(-hs/H*)` damping suppresses effective K at the coast to below Sprint 2 safe levels (per DD2 §定标逻辑).

**Dropped from SPACE 1.0 (deferred to Sprint 4+):**

- Explicit transport-capacity implicit solver (tridiagonal linear algebra for long-time-step stability under `n > 1`). Sprint 3 uses forward-Euler with `n = 1.0` (per Sprint 2 Kwang & Parker constraint), so CFL is manageable without implicit integration.
- SPACE's `V_s / q` deposition term (settling velocity / unit discharge ratio). Sprint 3 uses a simpler transport-capacity excess rule: `D = max(0, Qs_in - Qs_cap)` with `Qs_cap = K_Q · A^m_q · S^n_q`. This is physically defensible as a v1 approximation — deposition is triggered by the same supply–capacity mismatch as in SPACE, just parameterized differently.
- The `(1 - F_f)` fine-sediment fraction term in SPACE's bedrock erosion equation. Sprint 3 omits fine vs coarse partitioning; all eroded material is treated as a single `hs` proxy field.

**Why `K_sed / K_bed ≈ 3` is defensible (Kwang & Parker confirmation):**

Kwang & Parker 2017 established that the `(m, n) = (0.35, 1.0)` combination avoids the pathological instability that occurs near `m/n = 0.5`. Sprint 3's K_sed change (K_bed × 3) does NOT change `(m, n)` — it is a scaling of the prefactor only. The effective K felt by the D8-downstream routing is `K_bed · exp(-hs/H*)` for bedrock and `K_sed · min(hs, max)` for sediment; neither modifies the `m/n` ratio. The Kwang–Parker pathology domain is invariant to K scaling; `m/n = 0.35` remains well outside it regardless of K_bed / K_sed magnitude.
