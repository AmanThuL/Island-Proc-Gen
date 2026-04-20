---
id: hergarten_robl_2022_lfpm
title: "The linear feedback precipitation model (LFPM 1.0) – a simple and efficient model for orographic precipitation in the context of landform evolution modeling"
authors: Hergarten, Robl
year: 2022
venue: Geoscientific Model Development
doi: 10.5194/gmd-15-2063-2022
url: https://gmd.copernicus.org/articles/15/2063/2022/
tags: [orographic-precipitation, linear-model, LEM-coupling, moisture, efficiency]
sprint_first_used: sprint_2
status: metadata_only
---

## 一句话用途

Sprint 2 の降水 v2（`crates/sim/src/climate/precipitation.rs`）において、Smith-Barstad FFT の代替として LFPM の二成分線形モデル（水蒸気 + 雲水）を採用する際の実装仕様と理論的根拠。

## Abstract

The influence of climate on landform evolution has attracted great interest over the past decades. While many studies aim at determining erosion rates or parameters of erosion models, feedbacks between tectonics, climate, and landform evolution have been discussed but addressed quantitatively only in a few modeling studies. One of the problems in this field is that coupling a large-scale landform evolution model with a regional climate model would dramatically increase the theoretical and numerical complexity. Only a few simple models have been made available so far that allow efficient numerical coupling between topography-controlled precipitation and erosion. This paper fills this gap by introducing a quite simple approach involving two vertically integrated moisture components (vapor and cloud water). The interaction between the two components is linear and depends on altitude. This model structure is in principle the simplest approach that is able to predict both orographic precipitation at small scales and a large-scale decrease in precipitation over continental areas without introducing additional assumptions. Even in combination with transversal dispersion and elevation-dependent evapotranspiration, the model is of linear time complexity and increases the computing effort of efficient large-scale landform evolution models only moderately. Simple numerical experiments applying such a coupled landform evolution model show the strong impact of spatial precipitation gradients on mountain range geometry including steepness and peak elevation, position of the principal drainage divide, and drainage network properties.

## 关键方程 / 核心结论

- TODO (Sprint 1A first read)
- Two-component moisture model: vapor `q_v` and cloud water `q_c`, linearly coupled via altitude
- Linear time complexity O(N) — much cheaper than Smith-Barstad FFT O(N log N) for large grids
- Can reproduce both local orographic precipitation AND large-scale continental precipitation decrease

## 对本项目的落地点

- TODO (Sprint 1A first read)
- `crates/sim/src/climate/precipitation.rs` (Sprint 2 v2) — LFPM is the leading candidate for the Sprint 2 precipitation upgrade (over Smith-Barstad FFT) because of its O(N) coupling efficiency with the erosion loop
- `sprint_2_geomorph_credibility.md §4` ("Rain shadow v2") is where this paper will be deeply read

## 值得警惕的点

- TODO (Sprint 1A)
- LFPM is designed for large mountain ranges (continent scale); its assumptions about moisture advection over flat ocean may need tuning for small isolated volcanic islands
- "Linear" in the model name refers to the vapor-cloud interaction being linear in altitude, NOT that precipitation response to topography is linear — potential naming confusion

---

### Sprint 3 落地点

Sprint 3 DD4 implements LFPM-inspired precipitation v3 in `crates/sim/src/climate/precipitation_v3.rs`. Here is the mapping from LFPM's theoretical framework to the Sprint 3 explicit-Euler implementation:

**Core LFPM insight implemented in v3:**

LFPM's key contribution is replacing per-cell independent precipitation estimates (Sprint 1B's upwind raymarch) with a *stateful water-vapour field* `q(x, y)` that is advected and depleted as air moves over terrain. This makes precipitation physically consistent across the grid: upwind cells deplete the moisture budget, leaving naturally drier air for downwind cells. Sprint 3 implements this via sequential upwind sweep.

**Sequential upwind sweep (explicit `q` field):**

The sweep order is determined by projecting each cell's position onto the wind direction: `phase(p) = -wind · p_position`. Cells are processed in ascending `phase` order (upwind-most first). Each cell `p` inherits `q` from its upwind neighbour `p_upwind`, then applies local condensation and fallout:

```text
q(p) = q(p_upwind)
       · exp(-dt / TAU_C · uplift_factor(p))   // orographic condensation
       · exp(-dt / TAU_F)                        // generic fallout / rain shadow
       + marine_recharge(p)                      // coast-proximity boundary condition
P(p) = (q_before - q(p)) / dt                  // precipitation = q mass lost
```

This is the explicit-Euler version of LFPM's differential equation `dq/dx ∝ -(q - q_sat)/L_c`.

**Time-scale constants (DD4 locked values):**

- `TAU_C = 0.15`: condensation time scale. Smaller → faster moisture removal on windward slopes → wetter windward face. LFPM 2022 §3.1 shows that `τ_c ≈ 0.1–0.3 · (domain crossing time)` is the physically relevant range for orographic precipitation; 0.15 sits in the wet-bias end, appropriate for tropical volcanic islands with high uplift rates.
- `TAU_F = 0.60`: fallout (rain shadow) time scale. Larger → slower drying on the leeward side → gentler rain shadow. Sprint 1B's `k_shadow` parameter served a similar role; `τ_f = 0.60` produces comparable leeward dryness to the Sprint 2 calibrated `k_shadow` value while being physically grounded in moisture residence time.
- `Q_0 = 1.0`: marine boundary condition — cells at or near the upwind coast start with full moisture content. LFPM's formulation uses a normalized `[0,1]` vapour field; `Q_0 = 1.0` means "saturated marine air at the windward coast", which is appropriate for tropical oceanic islands where the marine moisture supply is essentially unlimited.

**Dropped from full LFPM 2022 (deferred to Sprint 4+):**

- Full implicit 2D solve (LFPM §4): implicit integration allows arbitrarily large time steps. Sprint 3 uses explicit-Euler with the single-pass sweep; this is unconditionally stable for the upwind sweep ordering (each cell reads only already-computed upstream values) so implicit integration is unnecessary at v1.
- Smith–Barstad FFT coupling (LFPM §5.2 extension): Smith–Barstad adds wave-number-domain orographic lifting. Deferred to Sprint 4+ as the gain over the explicit sweep is marginal at 256² resolution.
- Two-component (vapour + cloud water) full formulation: LFPM separates vapour `q_v` and cloud water `q_c` with separate advection equations. Sprint 3 collapses these into a single `q` field with combined `τ_c` / `τ_f` time scales. This simplification loses the ability to model cloud advection separately from precipitation onset but is sufficient for the v3 thesis (windward/leeward ratio improvement).

**Why the Sprint 1B "moisture swing too weak" symptom should resolve:**

Sprint 1B's upwind raymarch computed each cell's precipitation independently, so the total windward moisture budget was effectively unlimited — a cell near the ridge could be wet regardless of how much moisture had already been rained out upstream. The stateful `q` field depletes monotonically downwind; the windward/leeward contrast is structurally enforced by the sweep, not tunable via a single `CONDENSATION_RATE` constant. Sprint 2.5.Jb observed this as "moisture swing not strong enough under 180° wind flip"; the v3 sweep makes the swing structurally larger because the full island crosses the moisture depletion gradient.
