# Sprint 2 — Paper Pack Add-on

> **Sprint:** Sprint 2 — Geomorph Credibility
> **Spec ref:** `docs/design/sprints/sprint_2_geomorph_credibility.md` §7

---

## Sprint 2 硬读范围

Sprint 2 introduces stream-power incision + hillslope diffusion with an `ErosionOuterLoop` performing 10×10 explicit iterations. The five must-read papers below anchor the `(m, n)` parameter choices, calibration guidance, and carve→smooth sequencing logic. Two metadata-only entries (Braun 2023, Yuan 2019) are forward-references for the Sprint 4 GPU and implicit-solver pivot (see §11 "不做" rationale below).

---

## Must-Read Papers (5)

| Slug | Authors | Year | Purpose | Status |
|---|---|---|---|---|
| `chen_2014_lem_review` | Chen, Darbon, Morel | 2014 | erosion + hillslope diffusion coupling order; supports DD1 + DD2 "carve → smooth" sequencing | downloaded |
| `lague_2014_spim_evidence` | Lague | 2014 | SPIM empirical evidence for `(m, n) = (0.35, 1.0)` field grounding | downloaded |
| `whipple_tucker_1999_spim_dynamics` | Whipple, Tucker | 1999 | canonical `(m, n)` reference; explains why `n = 1` is the v1 linear stability choice | downloaded |
| `litwin_sklar_2025_hillslope_channel` | Litwin, Sklar | 2025 | hillslope diffusion vs stream-power relative contribution; calibrates D coefficient | on_disk |
| `kwang_parker_2017_mn_pathology` | Kwang, Parker | 2017 | **critical**: justifies why `m/n = 0.5` is pathological; why `(0.35, 1.0)` is the safe default | downloaded |

---

## Metadata-Only (2 — deferred to Sprint 4)

| Slug | Authors | Year | Purpose | Status |
|---|---|---|---|---|
| `braun_2023_implicit_spim` | Braun et al. | 2023 | implicit tridiagonal solver for SPIM; deferred to Sprint 4 GPU compute | metadata\_only |
| `yuan_2019_efficient_spim` | Yuan et al. | 2019 | efficient SPIM (high-order RK integration); deferred to Sprint 4 | metadata\_only |

---

## §11 — 为什么 Sprint 2 不上 implicit / 高效 SPIM

显式 Euler 4-substep hillslope diffusion + 10 个外层迭代在 v1 的 `K = 1e-3` 下稳定；implicit tridiagonal solver 引入了显著的实现复杂度，在 256² 分辨率下收益不足以抵消。单线程外层环 runtime 已在 200ms 以下的目标范围内（参考 Sprint 1C headless capture 性能基线），不需要 Yuan 2019 的高效 SPIM 技巧。Sprint 4 GPU 生产化时，如果 `n != 1` 非线性斜坡方案被采纳，implicit 会成为必须；until then，保持显式方案以最小化 Sprint 2 的引入复杂度。
