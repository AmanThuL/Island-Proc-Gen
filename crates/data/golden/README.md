# Golden Seeds

## Overview

Golden seeds are **(seed, preset)** pairs used as deterministic reproduction targets for island generation validation and regression testing.

Each golden seed entry specifies:
- A **seed value** (u64) that fully determines the pseudorandom sequence for generation
- A **preset name** that selects the island archetype (e.g., `volcanic_single`, `volcanic_twin`, `caldera`)

Sprint 0 ships three pairs:
- `(42, "volcanic_single")` — small single volcanic cone
- `(123, "volcanic_twin")` — dual volcanic centers
- `(777, "caldera")` — mature caldera morphology

## Reference Metrics (Sprint 1A+)

Reference metrics — per-seed snapshots of topography fields, flow routing outputs, and climate statistics — are deferred to **Sprint 1A** (when the topography and flow routing stages ship). At that time, this file will be extended with:

- Per-seed field snapshots (encoded as byte-level or cell-level deltas)
- Expected statistics (mean elevation, relief, depression area, etc.)
- Climate layer outputs (precipitation, evapotranspiration maps)

Once added, these metrics enable regression detection: if a code change alters the snapshot for a golden seed, the test failure highlights which seed was affected and by how much.

## Usage

Call `data::golden::load_golden_seeds()` to load this file at runtime or in tests.
