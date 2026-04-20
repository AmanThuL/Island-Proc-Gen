# Sprint 3 — Paper Pack Add-on

> **Sprint:** Sprint 3 — Sediment & Advanced Climate
> **Spec ref:** `docs/design/sprints/sprint_3_sediment_advanced_climate.md` §7

---

## Sprint 3 硬读范围

Sprint 3 introduces four interacting subsystems: sediment-aware SPACE-lite erosion (DD2), deposition-driven alluvial fan formation (DD3), LFPM-inspired precipitation v3 with a stateful water-vapour field (DD4), and trade-wind inversion layer fog hydrology coupling (DD5). A new CoastType v2 with 16-direction fetch integral and a `LavaDelta` fifth class (DD6) closes the volcanic coast credibility gap. Three new papers anchor these additions; five Core Pack / prior sprint pack papers require re-reading with Sprint 3 lenses.

---

## Must-Download Papers (3)

| # | Slug | Authors | Year | Venue | Purpose | Status |
|---|---|---|---|---|---|---|
| 20 | `ramalho_2013_volcanic_coast` | Ramalho, Quartau, Trenhaile, Mitchell, Woodroffe, Ávila | 2013 | Earth-Science Reviews | **Sprint 3 必读.** Comprehensive review of volcanic oceanic island coastal evolution — lava deltas, fresh benches, erosional asymmetry, coast-type classification logic. Primary reference for DD6 CoastType v2 fetch integral rationale and LavaDelta classification. | downloaded |
| 21 | `rodriguez_gonzalez_2022_lava_delta` | Rodriguez-Gonzalez, Fernandez-Turiel, Aulinas, Cabrera, Prieto-Torrell, Rodriguez, Guillou, Perez-Torrado | 2022 | Geomorphology | **Sprint 3 必读.** Dedicated lava delta study on El Hierro, Canary Islands; only paper providing quantified slope/proximity thresholds for lava delta morphology. Primary literature anchor for DD6 `S_LAVA_LOW/HIGH` and `R_LAVA` parameter selection. | downloaded |
| 22 | `bechon_2026_moorea` | Bechon, Hildenbrand, Pons, Dumont, Lachassagne, Sichoix | 2026 | J. Volcanology & Geothermal Research | Moorea case study: K/Ar-dated construction/destruction cycle of a highly eroded tropical volcanic island (French Polynesia). Sanity-check reference for `volcanic_eroded_ridge` / `volcanic_twin_old` archetypes. Metadata-only — deferred full read to Sprint 4.5 (curated screenshot sprint). | metadata_only (parking_lot) |

---

## Must-Reread Papers (5 — Core Pack + prior sprint packs)

| # | Slug | Authors | Year | Why now |
|---|---|---|---|---|
| 4 | `shobe_2017_space_v1` | Shobe, Tucker, Barnhart | 2017 | **DD2 main reference.** SPACE-lite double-equation (`E_bed` / `E_sed`) derives directly from SPACE 1.0. Re-read to fill "Sprint 3 落地点": what v1 keeps vs drops (no explicit transport-capacity implicit solve; `exp(-hs/H*)` damping retained; `K_sed/K_bed ≈ 3` ratio selection). |
| 8 | `hergarten_robl_2022_lfpm` | Hergarten, Robl | 2022 | **DD4 main reference.** LFPM v3 precipitation sweep directly implements the LFPM explicit-Euler approximation. Re-read to fill "Sprint 3 落地点": sequential upwind sweep ordering, `τ_c = 0.15` / `τ_f = 0.60` time-scale interpretation, `Q_0 = 1.0` marine boundary condition. |
| 16 | `bruijnzeel_2011_tmcf_hydromet` | Bruijnzeel, Mulligan, Scatena | 2011 | TMCF hydrometeorology synthesis. DD5 fog belt / inversion layer parameter justification: `inversion_z = 0.65 · max_relief`, `FOG_WATER_GAIN = 0.15`, `FOG_TO_SM_COUPLING = 0.40`. Order-of-magnitude grounding for fog-drip-to-soil-moisture contribution. |
| 17 | `bruijnzeel_2005_tmcf` | Bruijnzeel | 2005 | DD5 fog water → streamflow / soil moisture literature support. Bruijnzeel 2005 establishes that fog drip can supplement catchment water yield by 10–100+ % under TMCF conditions; supports the conservative coupling factor used in `SoilMoistureStage`. |
| 15 | `kwang_parker_2017_mn_pathology` | Kwang, Parker | 2017 | Confirm DD2 `K_bed/K_sed` split (`K_sed ≈ 3 × K_bed`) does not push effective `m/n` into the Kwang-Parker pathology domain. New `K_bed = 5e-3` is larger than Sprint 2 `K_SPIM = 1.5e-3` but `exp(-hs/H*)` damping reduces effective K at coast; `(m, n) = (0.35, 1.0)` unchanged. |

---

## §7 — 为什么 Sprint 3 不上 Full SPACE / Full LFPM

Full SPACE 2017 requires explicit transport-capacity-based deposition with an implicit tridiagonal solver for long-time-step stability under non-linear `n > 1`; the implementation complexity does not pay off at 256² / 10×10 outer loop. SPACE-lite retains the physics that matter for v1 (`exp(-hs/H*)` bedrock protection, `K_sed > K_bed` entrainment asymmetry, `Qs_cap` deposition) while deferring the implicit solver to Sprint 4 GPU productization. Full LFPM 2022 involves an implicit 2D solve and optionally couples Smith–Barstad FFT; Sprint 3 uses the explicit-Euler LFPM-inspired sequential sweep (DD4 option B), which is O(N) and provably convergent for the upwind sweep ordering. Smith–Barstad FFT deferred to Sprint 4+.

---

## Paper Task 验收清单

- [ ] `/docs/papers/sprint_packs/sprint_3.md` 存在且列全 3 必下 + 5 必复读
- [ ] `shobe_2017_space_v1.md` 有 "Sprint 3 落地点" 段指向 DD2 locked constants
- [ ] `hergarten_robl_2022_lfpm.md` 有 "Sprint 3 落地点" 段指向 DD4 `τ_c / τ_f / Q_0`
- [ ] `bruijnzeel_2011_tmcf_hydromet.md` 有 "Sprint 3 落地点" 段指向 DD5 `inversion_z / FOG_WATER_GAIN / FOG_TO_SM_COUPLING`
- [ ] `ramalho_2013_volcanic_coast.pdf` 存在于 `/docs/papers/`
- [ ] `rodriguez_gonzalez_2022_lava_delta.pdf` 存在于 `/docs/papers/`
- [ ] `bechon_2026_moorea.md` 在 `/docs/papers/parking_lot/` 含 DOI + why-later 一段
- [ ] 三个新 paper 各有 `notes/*.md` 含 frontmatter + abstract + Sprint 3 落地点
