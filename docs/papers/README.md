# Island Proc-Gen — Paper Knowledge Base

This directory holds all literature referenced by the Island-Proc-Gen project. Papers are organized into four layers so future sprints can quickly locate relevant reading and Claude Code can grep abstracts and key equations.

---

## Layer A — Core Pack (`core_pack/`)

Twelve foundational papers covering landscape evolution, orographic precipitation, vegetation dynamics, and procedural terrain generation. All stubs have frontmatter + abstract + one-sentence purpose filled in during Sprint 0.

**Sprint 0 hard target: ≥6 PDFs downloaded; Chen 2014 and Temme 2017 fully annotated.**

| # | Slug | Authors | Year | Status |
|---|---|---|---|---|
| 1 | `chen_2014_lem_review` | Chen, Darbon, Morel | 2014 | metadata\_only (Elsevier paywall) |
| 2 | `smith_barstad_2004_linear_orographic` | Smith, Barstad | 2004 | metadata\_only (AMS paywall) |
| 3 | `roe_2005_orographic_precipitation` | Roe | 2005 | downloaded (UW author page) |
| 4 | `shobe_2017_space_v1` | Shobe, Tucker, Barnhart | 2017 | downloaded (Copernicus GMD) |
| 5 | `chen_2023_budyko_similar_groups` | Chen et al. | 2023 | downloaded (HESS preprint) |
| 6 | `argles_2022_dgvm_balance` | Argles, Moore, Cox | 2022 | downloaded (PLOS Climate OA) |
| 7 | `fisher_2018_vegetation_demographics_esm` | Fisher et al. | 2018 | metadata\_only (Wiley paywall) |
| 8 | `hergarten_robl_2022_lfpm` | Hergarten, Robl | 2022 | metadata\_only (PDF >10 MB, retry Sprint 1B) |
| 9 | `gourbet_2024_hotspot_volcano_incision` | Gourbet et al. | 2024 | downloaded (GFZ institutional OA) |
| 10 | `genevaux_2013_hydrology_terrain` | Génevaux et al. | 2013 | downloaded (HAL author archive) |
| 11 | `lague_2014_spim_evidence` | Lague | 2014 | downloaded (Oregon State seminar) |
| 12 | `whipple_tucker_1999_spim_dynamics` | Whipple, Tucker | 1999 | downloaded (UChicago open) |

---

## Layer B — Sprint Packs (`sprint_packs/`)

One index file per sprint lists the papers added as sprint-specific reading. Sprint 0's add-on is Temme 2017 (stored in `core_pack/` because it is referenced everywhere, but indexed in `sprint_packs/sprint_0.md`).

| Sprint | File | Papers |
|---|---|---|
| Sprint 0 | `sprint_packs/sprint_0.md` | `temme_2017_lem_choose_use` |
| Sprint 1A | `sprint_packs/sprint_1a.md` | Litwin 2025, Kwang & Parker 2017 (to be added) |

---

## Layer C — Case Studies (`case_studies/`)

Island-specific literature on Réunion, Mauritius, Kauaʻi, Moorea, volcanic coasts, and cloud forests. Empty during Sprint 0; will be populated as the project reaches Sprint 2–3 calibration work.

---

## Layer D — Parking Lot (`parking_lot/`)

Citation-only references — DOI and title stored but no full text or note stub. Empty during Sprint 0.

---

## Note Stub Frontmatter Schema

Every file under `core_pack/` and `sprint_packs/*/notes/` must begin with this YAML frontmatter:

```yaml
---
id: <slug>                        # matches filename without .md
title: <full paper title>
authors: <last names, comma separated>
year: <yyyy>
venue: <journal or conference name>
doi: <doi string, no "https://doi.org/" prefix>  # omit if unknown
url: <canonical landing page>     # omit if unknown
pdf: core_pack/<slug>.pdf         # omit if not downloaded
tags: [<topic-tag>, ...]
sprint_first_used: <sprint_0 | sprint_1a | ...>
status: downloaded | metadata_only
---
```

Followed by these sections (in order):

1. `## 一句话用途` — one concrete sentence tying the paper to a specific module/Sprint in this repo
2. `## Abstract` — verbatim English abstract for grep-ability
3. `## 关键方程 / 核心结论` — 2–4 key equations or findings (may be `TODO (Sprint 1A first read)` for most Core Pack papers in Sprint 0)
4. `## 对本项目的落地点` — concrete `crates/...` file paths and sprint/task references (may be `TODO (Sprint 1A first read)` except Chen 2014 and Temme 2017)
5. `## 值得警惕的点` — assumptions, boundary conditions, misuse risks (may be `TODO (Sprint 1A)` in Sprint 0)
