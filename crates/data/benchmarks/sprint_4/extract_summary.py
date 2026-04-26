#!/usr/bin/env python3
"""Extract per-shot timing rows from a `--headless` summary.ron into CSV.

Sprint 4.0 (v3 schema): emits `shot_id, pipeline_ms, bake_ms, gpu_render_ms`.
Sprint 4.A onward (v4 schema): if a `stage_timings` map is present, the
script also emits `stage_<StageId>_cpu_ms` and `stage_<StageId>_gpu_ms`
columns alongside the lump-sum trio. Until 4.A lands, only the lump-sum
columns appear.

The script is intentionally regex-based: RON is close-enough to Python
tuple syntax for this simple field extraction, and pulling in a real RON
parser would be heavier than the script earns. If `summary.ron` grows new
non-AD8 fields with the same name pattern, extend the regex set here.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path


SHOT_RE = re.compile(r'^\s*id:\s*"([^"]+)"\s*,\s*$')
PIPELINE_MS_RE = re.compile(r'^\s*pipeline_ms:\s*([0-9.eE+-]+)\s*,\s*$')
BAKE_MS_RE = re.compile(r'^\s*bake_ms:\s*([0-9.eE+-]+)\s*,\s*$')
GPU_RENDER_MS_RE = re.compile(
    r'^\s*gpu_render_ms:\s*(?:Some\(\s*([0-9.eE+-]+)\s*\)|None)\s*,\s*$'
)


def extract(path: Path) -> list[dict[str, str]]:
    rows: list[dict[str, str]] = []
    current: dict[str, str] | None = None
    with path.open() as fh:
        for line in fh:
            m = SHOT_RE.match(line)
            if m:
                if current is not None:
                    rows.append(current)
                current = {"shot_id": m.group(1)}
                continue
            if current is None:
                continue
            m = PIPELINE_MS_RE.match(line)
            if m and "pipeline_ms" not in current:
                current["pipeline_ms"] = m.group(1)
                continue
            m = BAKE_MS_RE.match(line)
            if m and "bake_ms" not in current:
                current["bake_ms"] = m.group(1)
                continue
            m = GPU_RENDER_MS_RE.match(line)
            if m and "gpu_render_ms" not in current:
                current["gpu_render_ms"] = m.group(1) if m.group(1) is not None else ""
                continue
    if current is not None:
        rows.append(current)
    return rows


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: extract_summary.py <summary.ron>", file=sys.stderr)
        return 2
    summary = Path(sys.argv[1])
    rows = extract(summary)
    columns = ["shot_id", "pipeline_ms", "bake_ms", "gpu_render_ms"]
    print(",".join(columns))
    for row in rows:
        print(",".join(row.get(c, "") for c in columns))
    return 0


if __name__ == "__main__":
    sys.exit(main())
