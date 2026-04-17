---
id: temme_2017_lem_choose_use
title: "Developing, choosing and using landscape evolution models to inform field-based landscape reconstruction studies"
authors: Temme, Armitage, Attal, van Gorp, Coulthard, Schoorl
year: 2017
venue: Earth Surface Processes and Landforms
doi: 10.1002/esp.4162
url: https://onlinelibrary.wiley.com/doi/10.1002/esp.4162
tags: [landscape-evolution, model-selection, scope-fidelity, review, methodology]
sprint_first_used: sprint_0
status: metadata_only
---

## 一句话用途

Sprint 0 的 scope 决策依据：Temme 2017 提供的 "模型是为问题服务，不是越复杂越好" 框架直接约束了 Sprint 1A–3 的哪些 LEM 模块应该实现、哪些应该推迟——本项目的 `docs/design/sprints/sprint_0_scaffolding.md §5.3` 明确引用此原则。

## Abstract

Landscape evolution models (LEMs) are an increasingly popular resource for geomorphologists as they can operate as virtual laboratories where the implications of hypotheses about processes over human to geological timescales can be visualized at spatial scales from catchments to mountain ranges. Hypothetical studies for idealized landscapes have dominated, although model testing in real landscapes has also been undertaken. So far however, numerical landscape evolution models have rarely been used to aid field-based reconstructions of the geomorphic evolution of actual landscapes. To help make this use more common, we review numerical landscape evolution models from the point of view of model use in field reconstruction studies. We first give a broad overview of the main assumptions and choices made in many LEMs to help prospective users select models appropriate to their field situation. We then summarize for various timescales which data are typically available and which models are appropriate. Finally, we provide guidance on how to set up a model study as a function of available data and the type of research question.

## 关键方程 / 核心结论

- TODO (Sprint 1A first read) — Temme 2017 is methodology/review, not equation-heavy
- **Core principle (Sprint 0 hardcoded):** "Model complexity should match the question, not exceed it" — the guiding principle behind Sprint 0's decision to use synthetic volcanic cones rather than geological process-based initialization
- **Decision matrix (to be read in Sprint 1A):** Temme's table of "which processes to include given available constraints" will gate which erosion/climate modules enter Sprint 2 vs. get deferred

## 对本项目的落地点

**Sprint 0 / Scope decision already made:**
Temme 2017 is the primary justification for keeping Sprint 1A's LEM to a two-equation model (uplift + stream-power erosion only, no suspended sediment, no lithology variability, no tectonic flexure). The paper's argument that model components should be added only when corresponding field constraints exist maps directly to sprint_0 §2 "明确不做" (explicit non-goals) and sprint_1a §2 "锁定的设计决策."

**Sprint 1A — Scope gate for `crates/sim/src/geomorph/` and `crates/sim/src/hydro/`:**
Before implementing Task 1A.1–1A.6, the Temme decision matrix should be consulted to confirm that D8 routing + Planchon-Darboux pit fill + SPIM erosion is the appropriate complexity level for a procedural island generator (no real field data, no calibration measurements). Temme's guidance for "data-poor, idealised landscape" scenarios directly supports the Sprint 1A design decisions in §D1–D9.

**Sprint 2 — Credibility and coupling decisions:**
`docs/design/sprints/sprint_2_geomorph_credibility.md` §RD1–RD5 references Temme implicitly when deciding not to implement Braun 2023's implicit threshold SPIM or Yuan 2019's efficient SPIM. Temme's "tractability vs. fidelity" trade-off is the framework behind those deferrals. When Sprint 2 revisits `crates/sim/src/geomorph/` erosion coupling, re-read Temme §4 ("how to set up a model study").

**Sprint 3 — Credibility validation:**
`docs/design/sprints/sprint_2_geomorph_credibility.md` §1.8 mentions "before/after comparison" as a credibility deliverable. Temme 2017 is the source for what "model credibility" means when there is no field truth — using internal consistency checks (determinism, mass balance) rather than fit to real DEM data.

**Scope-vs-fidelity trade-off (architectural principle):**
The `SimulationPipeline` architecture in `crates/core/src/pipeline.rs` is designed so that stages can be added incrementally. Temme's principle that "each additional model component must be justified by the question" is the direct reason Sprint 0 registers only a `NoopStage` and defers all real stages to Sprint 1A+. This principle should be re-read each time a new `SimulationStage` is proposed.

## 值得警惕的点

- Temme 2017 is written for geomorphologists doing field reconstruction, not game/procedural developers. Calibration guidance (§5) assumes real DEM + dating data exist — our project deliberately operates without these, so Temme's validation framework must be adapted to "visual plausibility + internal consistency" rather than "fit to field evidence."
- TODO (Sprint 1A) — read §3 (model assumptions) and §4 (model setup) fully before implementing Sprint 1A stages.
- The paper was published in 2017; more recent implicit-solver LEMs (Braun 2023, Yuan 2019) are not covered. Cross-reference sprint_2 §RD3 for the up-to-date stability analysis.
