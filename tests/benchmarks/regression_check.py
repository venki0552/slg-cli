#!/usr/bin/env python3
"""Regression check for lore benchmarks.

Compares the latest benchmark run against the previous run on main.
Exits with code 1 if any metric degrades more than the threshold.

Usage:
  python tests/benchmarks/regression_check.py --threshold 0.05
"""

import argparse
import json
import sys
from pathlib import Path

RESULTS_DIR = Path("tests/benchmarks/results")


def load_latest_results(label: str) -> list[dict] | None:
    """Load the most recent results JSON for a given label."""
    result_dir = RESULTS_DIR / label
    if not result_dir.exists():
        return None

    json_files = sorted(result_dir.glob("*.json"), reverse=True)
    if not json_files:
        return None

    with open(json_files[0]) as f:
        return json.loads(f.read())


def compute_metrics(results: list[dict]) -> dict:
    """Compute aggregate metrics from evaluation results."""
    total = len(results)
    if total == 0:
        return {"completion_rate": 0, "avg_time": 0, "avg_rank": 0}

    found = sum(1 for r in results if r["found"])
    avg_time = sum(r["elapsed_seconds"] for r in results) / total
    ranks = [r["rank"] for r in results if r["rank"] > 0]
    avg_rank = sum(ranks) / len(ranks) if ranks else 0

    return {
        "completion_rate": found / total,
        "avg_time": avg_time,
        "avg_rank": avg_rank,
    }


def main():
    parser = argparse.ArgumentParser(description="Check for benchmark regressions")
    parser.add_argument("--threshold", type=float, default=0.05, help="Max allowed degradation (0.05 = 5%)")
    parser.add_argument("--label", default="lore", help="Results label to check")
    args = parser.parse_args()

    # Find the two most recent results
    result_dir = RESULTS_DIR / args.label
    if not result_dir.exists():
        print(f"No results found at {result_dir}")
        sys.exit(0)

    json_files = sorted(result_dir.glob("*.json"), reverse=True)
    if len(json_files) < 2:
        print("Not enough runs for regression check (need at least 2). Skipping.")
        sys.exit(0)

    with open(json_files[0]) as f:
        current = json.loads(f.read())
    with open(json_files[1]) as f:
        previous = json.loads(f.read())

    current_metrics = compute_metrics(current)
    previous_metrics = compute_metrics(previous)

    print("Regression Check")
    print("=" * 50)
    print(f"{'Metric':<25} {'Previous':>10} {'Current':>10} {'Delta':>10}")
    print("-" * 55)

    regressions = []

    # Completion rate — higher is better
    prev_cr = previous_metrics["completion_rate"]
    curr_cr = current_metrics["completion_rate"]
    delta_cr = curr_cr - prev_cr
    print(f"{'Completion Rate':<25} {prev_cr:>9.1%} {curr_cr:>9.1%} {delta_cr:>+9.1%}")
    if prev_cr > 0 and delta_cr < -args.threshold:
        regressions.append(f"Completion rate dropped by {abs(delta_cr):.1%}")

    # Average time — lower is better
    prev_t = previous_metrics["avg_time"]
    curr_t = current_metrics["avg_time"]
    if prev_t > 0:
        delta_t_pct = (curr_t - prev_t) / prev_t
    else:
        delta_t_pct = 0
    print(f"{'Avg Response Time':<25} {prev_t:>9.2f}s {curr_t:>9.2f}s {delta_t_pct:>+9.1%}")
    if delta_t_pct > args.threshold:
        regressions.append(f"Avg response time increased by {delta_t_pct:.1%}")

    # Average rank — lower is better
    prev_r = previous_metrics["avg_rank"]
    curr_r = current_metrics["avg_rank"]
    if prev_r > 0:
        delta_r_pct = (curr_r - prev_r) / prev_r
    else:
        delta_r_pct = 0
    print(f"{'Avg Rank':<25} {prev_r:>10.1f} {curr_r:>10.1f} {delta_r_pct:>+9.1%}")
    if delta_r_pct > args.threshold:
        regressions.append(f"Avg rank degraded by {delta_r_pct:.1%}")

    print()
    if regressions:
        print(f"FAIL: {len(regressions)} regression(s) detected (threshold: {args.threshold:.0%}):")
        for r in regressions:
            print(f"  - {r}")
        sys.exit(1)
    else:
        print(f"PASS: No regressions detected (threshold: {args.threshold:.0%})")
        sys.exit(0)


if __name__ == "__main__":
    main()
