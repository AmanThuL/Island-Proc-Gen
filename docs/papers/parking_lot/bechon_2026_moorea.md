---
id: bechon_2026_moorea
title: "Reconstructing the geological evolution of small tropical volcanic islands: Insights from the study case of Moorea (French Polynesia)"
authors: Bechon, Hildenbrand, Pons, Dumont, Lachassagne, Sichoix
year: 2026
venue: Journal of Volcanology and Geothermal Research
doi: 10.1016/j.jvolgeores.2026.108555
url: https://www.sciencedirect.com/science/article/pii/S0377027326000284
pdf_status: open_access_on_jvgr
tags: [volcanic-island, Moorea, French-Polynesia, geological-evolution, erosion, K-Ar-dating, tropical]
sprint_first_used: sprint_3
status: metadata_only
pack: sprint_3_parking_lot
---

# Bechon et al. 2026 — Moorea Geological Evolution (Parking Lot)

**Status:** metadata-only. Deferred deep read to Sprint 4.5 (Beauty / Demo / Shareability sprint) when curated screenshots and per-archetype visual sanity checks are on the agenda.

## Why deferred

Moorea is the canonical archetype "tropical volcanic island with highly eroded interior" — directly relevant to `volcanic_eroded_ridge` visual sanity check and secondarily to `volcanic_twin_old`. However, Sprint 3's done-definition does not include demo-quality screenshot production or per-archetype visual comparison (those are Sprint 4.5 responsibilities per roadmap vNext 2026-04-20). This paper's value is as a *visual reference target* once the Sprint 3.5 true-hex rendering and Sprint 4.5 curated-screenshot work begin — at that point, Bechon et al.'s geomorphological cross-sections and K/Ar-dated evolution stages provide a physical sanity-check framework for `volcanic_eroded_ridge` and `volcanic_twin_old`.

## Summary (from abstract)

Uses new K/Ar geochronology and fieldwork on Moorea (French Polynesia) to trace the island's volcanic construction-destruction cycle: initial shield-building phase ~1.85–1.70 Ma; major flank collapse ~1.64 Ma exposing submarine debris; rapid post-collapse rebuilding until ~1.35 Ma; followed by progressive erosional inversion of topography. The authors document successive construction and destruction episodes as a template for "mature deeply-eroded tropical volcanic island" geomorphology.

## Trigger for upgrade to full read

- Sprint 4.5 begins curated screenshot production for `volcanic_eroded_ridge` archetype
- `volcanic_twin_old` visual sanity check compares with Moorea-style erosional topography
- Any Sprint 3.5 hex-coast grammar work that needs real-island coast cross-section reference

## Cross-references

- `ramalho_2013_volcanic_coast.md` — complementary overview of volcanic island coast evolution; Moorea is implicitly in the "mature / old" stage of Ramalho's framework
- `gourbet_2024_hotspot_volcano_incision.md` (core pack) — hotspot volcano incision rates; Moorea's erosion stage is consistent with Gourbet's long-term incision patterns
- Sprint 3 DD6 `CoastType::LavaDelta` — Moorea at ~1.35 Ma post-shield has no active lava deltas (fully Mature/Old coast), confirming the `age_bias` gate in DD6
