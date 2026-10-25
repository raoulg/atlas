"""Compare indexed and uncached Atlas reads on an isolated copy of a repository."""

import argparse
import hashlib
import os
import shutil
import statistics
import subprocess
import tempfile
import time
from pathlib import Path


def sample(atlas: Path, repo: Path, output: Path, cache_home: Path, *, no_cache: bool) -> tuple[float, str]:
    command = [str(atlas), "--libraries", "0"]
    if no_cache:
        command.append("--no-cache")
    command.extend(["analyze", str(repo), "--out", str(output)])
    start = time.perf_counter()
    result = subprocess.run(command, capture_output=True, text=True, check=False, env={**os.environ, "XDG_CACHE_HOME": str(cache_home)})
    elapsed = time.perf_counter() - start
    if result.returncode:
        raise RuntimeError(f"{command!r} failed:\n{result.stderr}")
    return elapsed, hashlib.sha256(output.read_bytes()).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("atlas", type=Path, help="Atlas executable to measure")
    parser.add_argument("repo", type=Path, help="repository to copy as the benchmark subject")
    parser.add_argument("--rounds", type=int, default=5)
    args = parser.parse_args()
    if args.rounds < 1:
        parser.error("--rounds must be positive")
    atlas = args.atlas.resolve(strict=True)
    source = args.repo.resolve(strict=True)

    with tempfile.TemporaryDirectory(prefix="atlas-index-bench-") as directory:
        root = Path(directory)
        repo = root / "repo"
        shutil.copytree(source, repo, ignore=shutil.ignore_patterns(".git", ".atlas", "target", ".venv", "node_modules", "__pycache__"))
        (repo / ".git").mkdir(exist_ok=True)
        output = root / "graph.json"
        readings: dict[str, list[float]] = {"cold": [], "warm": [], "uncached": []}
        for _ in range(args.rounds):
            shutil.rmtree(repo / ".atlas" / "index", ignore_errors=True)
            cold, cold_hash = sample(atlas, repo, output, root / "cache", no_cache=False)
            entries = list((repo / ".atlas" / "index").rglob("*.json"))
            if not entries:
                raise RuntimeError("the cold reading created no index entry")
            entry_times = {path: path.stat().st_mtime_ns for path in entries}
            warm, warm_hash = sample(atlas, repo, output, root / "cache", no_cache=False)
            if any(path.stat().st_mtime_ns != mtime for path, mtime in entry_times.items()):
                raise RuntimeError("the warm reading rewrote an index entry")
            uncached, uncached_hash = sample(atlas, repo, output, root / "cache", no_cache=True)
            if len({cold_hash, warm_hash, uncached_hash}) != 1:
                raise RuntimeError("cold, warm and uncached graph bytes differ")
            for name, elapsed in (("cold", cold), ("warm", warm), ("uncached", uncached)):
                readings[name].append(elapsed)

    print(f"Atlas: {atlas}")
    print(f"Subject: {source} (isolated copy; --libraries 0)")
    print(f"Rounds: {args.rounds}; identical graph bytes in each round")
    for name, samples in readings.items():
        print(f"{name:8} median {statistics.median(samples):.3f}s; range {min(samples):.3f}–{max(samples):.3f}s")
    print(f"warm / uncached median: {statistics.median(readings['warm']) / statistics.median(readings['uncached']):.2f}")


if __name__ == "__main__":
    main()
