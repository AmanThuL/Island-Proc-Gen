#!/usr/bin/env bash
# Sprint 4 benchmark regen helper.
#
# Runs the 5 `--headless` baselines and extracts per-shot timing rows into
# CSV. After Sprint 4.A lands `--print-breakdown` and the v4
# `stage_timings` field, the script emits per-stage columns; until then it
# falls back to the v3 lump-sum trio (`pipeline_ms`, `bake_ms`,
# `gpu_render_ms`).
#
# Usage:
#   crates/data/benchmarks/sprint_4/regen.sh pre/cpu  # write into pre/cpu/<5>.csv
#   crates/data/benchmarks/sprint_4/regen.sh post/cpu # AFTER CPU snapshot
#   IPG_COMPUTE_BACKEND=gpu \
#     crates/data/benchmarks/sprint_4/regen.sh post/gpu
#
# The CSVs are *evidence artifacts*, not validation truth — wall-clock
# measurements drift across machines, OS schedulers, and GPU driver
# versions. Same-host before/after diffs are interpretable; cross-host
# comparisons are not (see README.md).

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

BUCKET="${1:-pre/cpu}"
OUT_DIR="crates/data/benchmarks/sprint_4/$BUCKET"
mkdir -p "$OUT_DIR"

BASELINES=(
    sprint_1a_baseline
    sprint_1b_acceptance
    sprint_2_erosion
    sprint_3_sediment_climate
    sprint_3_5_hex_surface
)

# Build once, run release; --print-breakdown is wired up at Sprint 4.A.
# Until then the CSVs are extracted from `summary.ron` directly via the
# Python helper below.
. "$HOME/.cargo/env" 2>/dev/null || true
cargo build -p app --release

for b in "${BASELINES[@]}"; do
    echo "=== regen: $b → $OUT_DIR/$b.csv ==="
    ./target/release/app --headless "crates/data/golden/headless/$b/request.ron" >/dev/null
    python3 crates/data/benchmarks/sprint_4/extract_summary.py \
        "crates/data/golden/headless/$b/summary.ron" \
        > "$OUT_DIR/$b.csv"
done

echo "Done — CSVs in $OUT_DIR/"
