#!/usr/bin/env python3
"""Create synthetic git repositories with planted ground-truth commits for benchmarking.

Usage:
  python scripts/create_synthetic_repos.py [--output-dir tests/benchmarks/repos]

Creates three repos: synthetic/small (50 commits), synthetic/medium (500), synthetic/large (5000).
~20% of commits contain planted answers with unique IDs ([BENCH-xxxxxxxx]).
Ground truth JSON is written alongside each repo.
"""

import argparse
import json
import os
import random
import string
import subprocess
import uuid
from datetime import datetime, timedelta
from pathlib import Path

REPO_SIZES = {
    "small": 50,
    "medium": 500,
    "large": 5000,
}

# Realistic commit message templates (no planted answers)
FILLER_MESSAGES = [
    "fix: resolve null pointer in user service",
    "feat: add pagination to listing endpoint",
    "refactor: extract validation logic to utils",
    "chore: update dependencies to latest",
    "docs: update API documentation for v2",
    "test: add missing unit tests for auth module",
    "perf: optimize database query for report generation",
    "fix: handle edge case in date parsing",
    "feat: implement webhook retry mechanism",
    "chore: clean up unused imports",
    "fix: correct off-by-one error in pagination",
    "feat: add user preference storage",
    "refactor: simplify error handling in middleware",
    "docs: add deployment guide",
    "test: integration tests for payment flow",
    "fix: prevent duplicate event processing",
    "feat: add rate limiting to public API",
    "chore: configure eslint rules",
    "perf: cache frequently accessed config values",
    "fix: timezone handling in scheduled tasks",
    "feat: add CSV export for reports",
    "refactor: move constants to dedicated module",
    "fix: memory leak in connection pool",
    "docs: update changelog for v1.2",
    "test: add stress tests for concurrent access",
    "feat: implement soft delete for resources",
    "chore: bump minimum Node version to 20",
    "fix: race condition in batch processing",
    "perf: reduce unnecessary re-renders",
    "feat: add search filters for date range",
]

# Planted answer templates — these have specific, retrievable facts
PLANTED_TEMPLATES = [
    {
        "subject": "Set payment retry limit to {value} [BENCH-{uid}]",
        "body": "Payment processor SLA requires max {value} retry attempts [BENCH-{uid}].\n"
                "Exceeding {value} causes duplicate charge risk per contract section 4.2.\n"
                "Tested with QA team on 2024-08-15. Approved by @finance.\n"
                "Reference: BENCH-{uid}",
        "query": "Why is the retry limit set to {value}?",
        "required_facts": ["SLA", "{value}", "retry", "duplicate charge"],
        "category": "history_retrieval",
        "value_choices": ["3", "5", "7"],
    },
    {
        "subject": "Reduce JWT expiry to {value} hours for SOC2 [BENCH-{uid}]",
        "body": "SOC2 compliance audit finding: tokens must expire within {value} hours [BENCH-{uid}].\n"
                "Previous 30-day expiry violated section CC6.1.\n"
                "Security team approved change on 2024-09-01.\n"
                "Reference: BENCH-{uid}",
        "query": "Why was JWT expiry reduced to {value} hours?",
        "required_facts": ["SOC2", "{value}", "expiry", "CC6.1"],
        "category": "compliance",
        "value_choices": ["1", "4", "24"],
    },
    {
        "subject": "Switch from REST to gRPC for service mesh [BENCH-{uid}]",
        "body": "Performance testing showed {value}x throughput improvement with gRPC [BENCH-{uid}].\n"
                "REST json serialization was bottleneck at >10k req/s.\n"
                "Protobuf schema reduces payload size by 70%.\n"
                "Reference: BENCH-{uid}",
        "query": "Why did we switch from REST to gRPC?",
        "required_facts": ["throughput", "gRPC", "serialization", "protobuf"],
        "category": "architecture_decision",
        "value_choices": ["3", "5", "8"],
    },
    {
        "subject": "Cap batch size at {value} for memory safety [BENCH-{uid}]",
        "body": "OOM incident on 2024-07-22 caused by unbounded batch processing [BENCH-{uid}].\n"
                "Memory profiling showed {value} items keeps RSS under 512MB.\n"
                "Larger batches risk OOM kill under k8s resource limits.\n"
                "Reference: BENCH-{uid}",
        "query": "Why is the batch size capped at {value}?",
        "required_facts": ["OOM", "{value}", "memory", "512MB"],
        "category": "history_retrieval",
        "value_choices": ["100", "500", "1000"],
    },
    {
        "subject": "Add circuit breaker with {value}s timeout [BENCH-{uid}]",
        "body": "Cascading failure on 2024-06-15 took down 3 services [BENCH-{uid}].\n"
                "Circuit breaker pattern prevents cascade with {value}s timeout.\n"
                "After {value}s, requests fail fast instead of queuing.\n"
                "Reference: BENCH-{uid}",
        "query": "Why was the circuit breaker timeout set to {value} seconds?",
        "required_facts": ["cascading failure", "{value}", "circuit breaker", "fail fast"],
        "category": "incident_response",
        "value_choices": ["5", "10", "30"],
    },
]

FILE_NAMES = [
    "src/main.py", "src/auth/handler.py", "src/api/routes.py",
    "src/models/user.py", "src/utils/helpers.py", "src/config.py",
    "src/services/payment.py", "src/services/notification.py",
    "tests/test_auth.py", "tests/test_api.py", "README.md",
    "src/middleware/rate_limit.py", "src/services/search.py",
]


def random_file_content(length: int = 200) -> str:
    """Generate random 'code-like' content."""
    lines = []
    for _ in range(length // 20):
        indent = "    " * random.randint(0, 2)
        word_count = random.randint(3, 8)
        words = [random.choice(string.ascii_lowercase) * random.randint(2, 8) for _ in range(word_count)]
        lines.append(f"{indent}{' '.join(words)}")
    return "\n".join(lines)


def create_repo(output_dir: Path, name: str, num_commits: int) -> list[dict]:
    """Create a synthetic git repo and return ground truth entries."""
    repo_path = output_dir / "synthetic" / name
    repo_path.mkdir(parents=True, exist_ok=True)

    env = os.environ.copy()
    env["GIT_AUTHOR_NAME"] = "Benchmark Bot"
    env["GIT_AUTHOR_EMAIL"] = "bench@slg.dev"
    env["GIT_COMMITTER_NAME"] = "Benchmark Bot"
    env["GIT_COMMITTER_EMAIL"] = "bench@slg.dev"

    subprocess.run(["git", "init"], cwd=repo_path, capture_output=True, env=env, check=True)

    # Determine planted commit indices (~20% of commits, spread evenly)
    num_planted = max(1, num_commits // 5)
    planted_indices = set(random.sample(range(num_commits), min(num_planted, len(PLANTED_TEMPLATES) * 3)))

    ground_truth = []
    planted_template_idx = 0
    base_date = datetime(2024, 1, 1, 10, 0, 0)

    for i in range(num_commits):
        commit_date = base_date + timedelta(hours=i * 2)
        date_str = commit_date.strftime("%Y-%m-%dT%H:%M:%S")
        env["GIT_AUTHOR_DATE"] = date_str
        env["GIT_COMMITTER_DATE"] = date_str

        # Pick a random file to modify
        file_name = random.choice(FILE_NAMES)
        file_path = repo_path / file_name
        file_path.parent.mkdir(parents=True, exist_ok=True)
        file_path.write_text(random_file_content())

        subprocess.run(["git", "add", "."], cwd=repo_path, capture_output=True, env=env, check=True)

        if i in planted_indices:
            # Planted commit with ground truth
            template = PLANTED_TEMPLATES[planted_template_idx % len(PLANTED_TEMPLATES)]
            planted_template_idx += 1
            uid = uuid.uuid4().hex[:8]
            value = random.choice(template["value_choices"])

            subject = template["subject"].format(value=value, uid=uid)
            body = template["body"].format(value=value, uid=uid)
            message = f"{subject}\n\n{body}"

            subprocess.run(
                ["git", "commit", "-m", message, "--allow-empty"],
                cwd=repo_path, capture_output=True, env=env, check=True,
            )

            # Get the commit hash
            result = subprocess.run(
                ["git", "rev-parse", "HEAD"],
                cwd=repo_path, capture_output=True, text=True, env=env, check=True,
            )
            commit_hash = result.stdout.strip()

            ground_truth.append({
                "id": f"task-{len(ground_truth) + 1:03d}",
                "category": template["category"],
                "query": template["query"].format(value=value),
                "repo": f"synthetic/{name}",
                "ground_truth": {
                    "commit_hash": commit_hash,
                    "required_facts": [f.format(value=value) for f in template["required_facts"]],
                    "unique_id": f"BENCH-{uid}",
                },
            })
        else:
            # Filler commit
            message = random.choice(FILLER_MESSAGES)
            subprocess.run(
                ["git", "commit", "-m", message, "--allow-empty"],
                cwd=repo_path, capture_output=True, env=env, check=True,
            )

    return ground_truth


def main():
    parser = argparse.ArgumentParser(description="Create synthetic benchmark repos")
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path("tests/benchmarks/repos"),
        help="Output directory for repos",
    )
    args = parser.parse_args()

    all_ground_truth = []

    for name, size in REPO_SIZES.items():
        print(f"Creating synthetic/{name} ({size} commits)...")
        truth = create_repo(args.output_dir, name, size)
        all_ground_truth.extend(truth)
        print(f"  → {len(truth)} planted ground-truth commits")

    # Write ground truth JSON
    truth_path = args.output_dir / "ground_truth.json"
    truth_path.write_text(json.dumps(all_ground_truth, indent=2))
    print(f"\nGround truth written to {truth_path} ({len(all_ground_truth)} tasks)")


if __name__ == "__main__":
    main()
