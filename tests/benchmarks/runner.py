#!/usr/bin/env python3
"""Benchmark runner for lore.

Runs retrieval tasks against synthetic repos and measures:
- Task completion (correct commit found?)
- Token usage
- Wall clock time
- Answer rank (position of correct commit in results)

Usage:
  python tests/benchmarks/runner.py --quick --repos synthetic/small
  python tests/benchmarks/runner.py --repos synthetic/medium
  python tests/benchmarks/runner.py --all
"""

import argparse
import json
import os
import subprocess
import sys
import time
from datetime import datetime
from pathlib import Path

REPO_BASE = Path("tests/benchmarks/repos")
RESULTS_DIR = Path("tests/benchmarks/results")


def run_lore_query(binary: str, repo_path: str, query: str, timeout: int = 30) -> dict:
    """Run a lore why query and measure results."""
    start = time.time()

    try:
        result = subprocess.run(
            [binary, "why", query, "--format", "json", "--limit", "10"],
            cwd=repo_path,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        elapsed = time.time() - start

        if result.returncode != 0:
            return {
                "success": False,
                "error": result.stderr.strip(),
                "elapsed_seconds": elapsed,
                "results": [],
            }

        try:
            output = json.loads(result.stdout)
            return {
                "success": True,
                "elapsed_seconds": elapsed,
                "results": output.get("results", []),
                "raw_output_bytes": len(result.stdout),
            }
        except json.JSONDecodeError:
            return {
                "success": False,
                "error": "Invalid JSON output",
                "elapsed_seconds": elapsed,
                "results": [],
            }

    except subprocess.TimeoutExpired:
        return {
            "success": False,
            "error": "Timeout",
            "elapsed_seconds": timeout,
            "results": [],
        }


def evaluate_result(query_result: dict, ground_truth: dict) -> dict:
    """Evaluate a query result against ground truth."""
    gt = ground_truth["ground_truth"]
    target_hash = gt["commit_hash"]
    unique_id = gt["unique_id"]

    found = False
    rank = -1

    for i, result in enumerate(query_result.get("results", [])):
        commit_hash = result.get("hash", result.get("commit_hash", ""))
        message = result.get("message", result.get("subject", ""))

        if target_hash.startswith(commit_hash) or commit_hash.startswith(target_hash):
            found = True
            rank = i + 1
            break
        if unique_id in message:
            found = True
            rank = i + 1
            break

    return {
        "task_id": ground_truth["id"],
        "query": ground_truth["query"],
        "category": ground_truth["category"],
        "found": found,
        "rank": rank,
        "elapsed_seconds": query_result.get("elapsed_seconds", 0),
        "num_results": len(query_result.get("results", [])),
        "output_bytes": query_result.get("raw_output_bytes", 0),
    }


def find_lore_binary() -> str:
    """Find the lore binary."""
    # Try release build first, then debug
    for profile in ["release", "debug"]:
        for ext in ["", ".exe"]:
            path = Path(f"target/{profile}/lore{ext}")
            if path.exists():
                return str(path.resolve())

    # Try PATH
    import shutil
    which = shutil.which("lore")
    if which:
        return which

    print("Error: lore binary not found. Run 'cargo build --release' first.", file=sys.stderr)
    sys.exit(1)


def run_benchmark(repo_filter: str, quick: bool = False) -> list[dict]:
    """Run benchmark tasks for matching repos."""
    ground_truth_path = REPO_BASE / "ground_truth.json"
    if not ground_truth_path.exists():
        print(f"Error: {ground_truth_path} not found. Run create_synthetic_repos.py first.", file=sys.stderr)
        sys.exit(1)

    with open(ground_truth_path) as f:
        all_tasks = json.loads(f.read())

    # Filter tasks by repo
    tasks = [t for t in all_tasks if repo_filter in t["repo"]]
    if quick:
        tasks = tasks[:5]

    if not tasks:
        print(f"No tasks found matching repo filter '{repo_filter}'", file=sys.stderr)
        sys.exit(1)

    binary = find_lore_binary()
    print(f"Using binary: {binary}")
    print(f"Running {len(tasks)} tasks...")

    # First, ensure repo is indexed
    first_repo = REPO_BASE / tasks[0]["repo"]
    if first_repo.exists():
        print(f"Indexing {first_repo}...")
        subprocess.run(
            [binary, "init", "--silent"],
            cwd=str(first_repo),
            capture_output=True,
            timeout=120,
        )

    evaluations = []
    for i, task in enumerate(tasks):
        repo_path = str(REPO_BASE / task["repo"])
        print(f"  [{i + 1}/{len(tasks)}] {task['id']}: {task['query'][:60]}...")

        result = run_lore_query(binary, repo_path, task["query"])
        evaluation = evaluate_result(result, task)
        evaluations.append(evaluation)

        status = "✓" if evaluation["found"] else "✗"
        print(f"    {status} rank={evaluation['rank']} time={evaluation['elapsed_seconds']:.2f}s")

    return evaluations


def write_results(evaluations: list[dict], label: str) -> Path:
    """Write results to JSON and markdown summary."""
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    out_dir = RESULTS_DIR / label
    out_dir.mkdir(parents=True, exist_ok=True)

    # Write JSON
    json_path = out_dir / f"{timestamp}.json"
    with open(json_path, "w") as f:
        json.dump(evaluations, f, indent=2)

    # Write markdown summary
    total = len(evaluations)
    found = sum(1 for e in evaluations if e["found"])
    avg_time = sum(e["elapsed_seconds"] for e in evaluations) / max(total, 1)
    avg_rank = sum(e["rank"] for e in evaluations if e["rank"] > 0) / max(found, 1)

    md_path = RESULTS_DIR / "latest.md"
    with open(md_path, "w") as f:
        f.write(f"# Benchmark Results — {label}\n\n")
        f.write(f"**Date**: {datetime.now().isoformat()}\n\n")
        f.write(f"| Metric | Value |\n")
        f.write(f"|--------|-------|\n")
        f.write(f"| Tasks | {total} |\n")
        f.write(f"| Completion Rate | {found}/{total} ({100 * found / max(total, 1):.0f}%) |\n")
        f.write(f"| Avg Response Time | {avg_time:.2f}s |\n")
        f.write(f"| Avg Rank (when found) | {avg_rank:.1f} |\n")
        f.write(f"\n### Per-Task Results\n\n")
        f.write(f"| Task | Category | Found | Rank | Time |\n")
        f.write(f"|------|----------|-------|------|------|\n")
        for e in evaluations:
            status = "✓" if e["found"] else "✗"
            f.write(f"| {e['task_id']} | {e['category']} | {status} | {e['rank']} | {e['elapsed_seconds']:.2f}s |\n")

    print(f"\nResults written to {json_path}")
    print(f"Summary written to {md_path}")
    return json_path


def main():
    parser = argparse.ArgumentParser(description="Run lore benchmarks")
    parser.add_argument("--repos", default="synthetic/small", help="Repo filter (e.g. synthetic/small)")
    parser.add_argument("--quick", action="store_true", help="Run only 5 tasks")
    parser.add_argument("--all", action="store_true", help="Run all repos")
    parser.add_argument("--label", default="lore", help="Label for results (lore or baseline)")
    args = parser.parse_args()

    if args.all:
        for size in ["small", "medium", "large"]:
            evaluations = run_benchmark(f"synthetic/{size}", quick=args.quick)
            write_results(evaluations, f"{args.label}/{size}")
    else:
        evaluations = run_benchmark(args.repos, quick=args.quick)
        write_results(evaluations, args.label)


if __name__ == "__main__":
    main()
