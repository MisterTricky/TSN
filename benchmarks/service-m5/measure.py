#!/usr/bin/env python3
"""Compare the M5 fixture's Bun and tsn source-core workloads on Linux."""

import argparse
import json
import platform
import shlex
import statistics
import subprocess
import time
from pathlib import Path


def run(command: str, name: str, root: Path) -> tuple[float, int, int, float]:
    started = time.perf_counter()
    result = subprocess.run(
        ["/usr/bin/time", "-f", "M5_RSS\t%M", *shlex.split(command)],
        cwd=root,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    elapsed = time.perf_counter() - started
    if result.returncode:
        raise RuntimeError(
            f"{name} exited {result.returncode}: {result.stderr.decode(errors='replace')}"
        )
    lines = result.stdout.decode("utf-8").strip().splitlines()
    if not lines:
        raise RuntimeError(f"{name} produced no output")
    if name == "Bun":
        summary = json.loads(lines[-1])
        checksum = int(summary["checksum"])
        if int(summary["iterations"]) != 100000:
            raise RuntimeError("Bun runner did not execute 100,000 operations")
        service_seconds = float(summary["elapsedMs"]) / 1000.0
    else:
        if len(lines) != 2:
            raise RuntimeError(f"expected tsn checksum and internal timing, got {lines!r}")
        checksum = int(lines[0])
        service_seconds = float(lines[1]) / 1000.0
        if checksum != 44540000:
            raise RuntimeError(f"unexpected tsn checksum: {checksum}")
    timing = result.stderr.decode().strip().splitlines()[-1].split("\t")
    if len(timing) != 2 or timing[0] != "M5_RSS":
        raise RuntimeError(f"could not parse GNU time output: {result.stderr!r}")
    return service_seconds, int(timing[1]), checksum, elapsed


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bun", required=True)
    parser.add_argument("--tsn", required=True)
    parser.add_argument("--runs", type=int, default=10)
    parser.add_argument("--raw-out", type=Path, default=Path(__file__).with_name("RESULTS-RAW.json"))
    args = parser.parse_args()
    if args.runs < 2:
        parser.error("--runs must be at least 2")
    root = Path(__file__).resolve().parents[2]
    commands = [("Bun", args.bun), ("tsn", args.tsn)]
    samples: dict[str, list[tuple[float, int, float]]] = {name: [] for name, _ in commands}
    expected_checksum = None

    for name, command in commands:
        _, _, checksum, _ = run(command, name, root)
        if expected_checksum is None:
            expected_checksum = checksum
        elif checksum != expected_checksum:
            raise RuntimeError(f"{name} checksum differs: {checksum} != {expected_checksum}")

    for index in range(args.runs):
        order = commands[index % 2 :] + commands[: index % 2]
        for name, command in order:
            elapsed, rss, checksum, process_seconds = run(command, name, root)
            if checksum != expected_checksum:
                raise RuntimeError(f"{name} checksum changed during measurement")
            samples[name].append((elapsed, rss, process_seconds))

    for name, _ in commands:
        times = [sample[0] for sample in samples[name]]
        rss = [sample[1] for sample in samples[name]]
        process_times = [sample[2] for sample in samples[name]]
        print(f"{name}: samples_service_seconds_rss_kib_process_seconds={samples[name]}")
        print(
            f"{name}: median_service_seconds={statistics.median(times):.6f} "
            f"median_process_seconds={statistics.median(process_times):.6f} "
            f"max_rss_kib={max(rss)}"
        )

    bun_median = statistics.median(sample[0] for sample in samples["Bun"])
    tsn_median = statistics.median(sample[0] for sample in samples["tsn"])
    bun_rss = max(sample[1] for sample in samples["Bun"])
    tsn_rss = max(sample[1] for sample in samples["tsn"])
    speedup = bun_median / tsn_median
    print(f"tsn_throughput_vs_bun={speedup:.3f}x tsn_to_bun_rss_ratio={tsn_rss / max(bun_rss, 1):.3f}x")

    raw = {
        "platform": platform.platform(),
        "runs": args.runs,
        "iterations_per_run": 100000,
        "commands": dict(commands),
        "checksum": expected_checksum,
        "samples_service_seconds_rss_kib_process_seconds": samples,
        "summary": {
            "bun_median_seconds": bun_median,
            "tsn_median_seconds": tsn_median,
            "bun_max_rss_kib": bun_rss,
            "tsn_max_rss_kib": tsn_rss,
            "throughput_speedup": speedup,
        },
    }
    args.raw_out.write_text(json.dumps(raw, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
