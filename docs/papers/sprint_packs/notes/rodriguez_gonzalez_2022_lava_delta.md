---
id: rodriguez_gonzalez_2022_lava_delta
title: "Lava deltas, a key landform in oceanic volcanic islands: El Hierro, Canary Islands"
authors: Rodriguez-Gonzalez, Fernandez-Turiel, Aulinas, Cabrera, Prieto-Torrell, Rodriguez, Guillou, Perez-Torrado
year: 2022
venue: Geomorphology
doi: 10.1016/j.geomorph.2022.108427
url: https://www.sciencedirect.com/science/article/pii/S0169555X22003208
pdf: rodriguez_gonzalez_2022_lava_delta.pdf
tags: [lava-delta, volcanic-island, coastal-geomorphology, El-Hierro, Canary-Islands, slope-threshold, eruption-volume]
sprint_first_used: sprint_3
status: downloaded
---

## 一句话用途

Sprint 3 DD6 `CoastType::LavaDelta` 分类的唯一 dedicated 文献锚点：基于 El Hierro 17 次喷发的 lava delta 案例，本文提供了坡度范围、距火山中心距离、岛龄依赖性等关键定量特征，直接 ground `S_LAVA_LOW/HIGH` 和 `R_LAVA` 常量选择。

## Abstract

Marine and subaerial erosion of volcanic ocean islands form coastal cliffs and shore platforms. These are modified when lava flows extend beyond the cliffs, creating lava deltas. This study examines 17 eruptions on El Hierro (Canary Islands) that produced lava deltas. The Montaña del Tesoro eruption (~1050 years ago) is used as a primary case study — a Strombolian event producing a cinder cone, pyroclastic deposits, and lava flows that reached the ocean. Analysis of lava delta morphology, volume, and erosional fate shows that marine erosion removed approximately 9% of the erupted lava flow volume, compared with only 1% by fluvial erosion, demonstrating that coastal processes dominate the destruction of fresh lava deltas on young volcanic islands. The study maps lava delta occurrence, measures slope and distance to volcanic vent, and characterises the transition from delta to mature coast as a function of age, wave exposure, and eruptive volume.

## 关键方程 / 核心结论

- Lava deltas on El Hierro cluster at distances ≤ ~8 km from volcanic centers (island radius ~18 km → ~0.44 normalized); but active vents in the case study are within ~5 km of coast → yields normalized distance `R_LAVA ≈ 0.25–0.35` for relevant occurrences.
- Lava delta slopes are low-gradient (lower than typical sea cliffs), in the range 1–6° based on measured delta faces; in normalized slope terms this corresponds to roughly `[0.02, 0.10]` depending on grid resolution and max_relief.
- Marine erosion dominates (9 % volume loss vs 1 % fluvial) → LavaDelta is a transient "young coast" feature; Mature/Old archetypes should show zero LavaDelta (replaced by RockyHeadland or Beach through wave erosion).
- Lava delta identification requires age constraint (young eruption) + proximity to vent + low coastal slope — all three conditions must be met simultaneously.

## 対本項目의 落地点

### Sprint 3 落地点

Rodriguez-Gonzalez 2022 が DD6 の `CoastType::LavaDelta` 判別ロジックの定量アンカーを提供する（`crates/sim/src/geomorph/coast_type.rs`）：

1. **`S_LAVA_LOW = 0.03 / S_LAVA_HIGH = 0.10`の根拠：** 本文の El Hierro ケーススタディにおける lava delta 面の測定傾斜は 1–6° の範囲。Sprint 3 の normalized slope（`derived.slope[p]`）は `tan(θ)` ベースで、1° ≈ 0.017、6° ≈ 0.105 に対応。DD6 の `[0.03, 0.10]` ウィンドウはこの実測範囲を安全マージン付きでカバーする。

2. **`R_LAVA = 0.30`の根拠：** 本文の active vent から lava delta end point までの距離は island scale の 25–45 % 相当。`R_LAVA = 0.30` は保守側（距火山中心 0.30 normalized）で、young island の「海岸に近い fire center」シナリオをカバーしながら island 全域に LavaDelta が広がる非物理ケースを除外する。

3. **`age_bias > 0 (Young)` の必要条件：** 本文は「lava delta は活発な young volcanism の産物」と明確に述べており、Mature / Old island では marine erosion によって既に平坦化される。DD6 の `if age_bias > 0.0` ゲートは直接この観察から導出される。`volcanic_caldera_young` / `volcanic_single` (Young) のみで LavaDelta > 0 % になること（`volcanic_twin_old` / `volcanic_eroded_ridge` では = 0 %）はこの文献に物理根拠を持つ。

4. **`ValidationStage` の `coast_type_v2_well_formed` invariant：** "LavaDelta ONLY on Young-age archetypes" の assertion が test に焼き込まれており、この文献の "age-dependent transience" 観察と 1:1 で対応する。

## 値得警惕的点

- El Hierro は Canary Islands の中でも面積が小さい（268 km²）、活動的な若い島。Sprint 3 の `volcanic_single` / `volcanic_caldera_young` archetype とスケールは近いが、フランス領ポリネシア型（Moorea / Bora Bora）の古い侵食された島とは異なる。パラメータは Canary Islands 文脈で選定された。
- 本文の slope 測定は野外の実際の lava delta 前面斜面（subaerial + submarine）であり、Sprint 3 の `derived.slope[p]` は 2D DEM 勾配（8-近傍 Sobel 近似）とは異なる概念。normalized slope 変換は概算であり、v2 物理較正での再チェックが望ましい。
- 本文には 17 eruption の統計があるが、全ての lava delta が現存しているわけではなく、多くは既に erosion 消失している。「観察できる lava delta」のバイアス（young + voluminous eruptions が残りやすい）に注意。
