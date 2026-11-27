#!/usr/bin/env python3
"""Measure standalone record-stream runners with GNU time on Linux."""

import argparse
import hashlib
import json
import platform
import shlex
import statistics
import subprocess
import time
from pathlib import Path


def run(command: str, input_path: Path) -> tuple[float, int, str]:
    started = time.perf_counter()
    with input_path.open("rb") as source:
        result = subprocess.run(
            ["/usr/bin/time", "-f", "TSN_RSS\t%M", *shlex.split(command)],
            stdin=source,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
    if result.returncode:
        raise RuntimeError(
            f"{command!r} exited {result.returncode}: "
            f"{result.stderr.decode(errors='replace')}"
        )
    elapsed = time.perf_counter() - started
    timing = result.stderr.decode().strip().splitlines()[-1].split("\t")
    if len(timing) != 2 or timing[0] != "TSN_RSS":
        raise RuntimeError(f"could not parse GNU time output: {result.stderr!r}")
    return elapsed, int(timing[1]), result.stdout.decode("ascii").strip()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--c", required=True)
    parser.add_argument("--tsn", required=True)
    parser.add_argument("--bun", help="optional informational Bun baseline")
    parser.add_argument("--input", type=Path, default=Path(__file__).with_name("input.bin"))
    parser.add_argument("--runs", type=int, default=10)
    parser.add_argument("--raw-out", type=Path, help="write raw samples and environment as JSON")
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    if not args.input.is_file():
        parser.error(f"input does not exist: {args.input}")

    commands = [("C", args.c), ("tsn", args.tsn)]
    if args.bun:
        commands.append(("Bun", args.bun))
    expected_output = None
    samples: dict[str, list[tuple[float, int]]] = {name: [] for name, _ in commands}

    # Discard one untimed process per implementation to avoid measuring first-run setup.
    for name, command in commands:
        _, _, output = run(command, args.input)
        if expected_output is None:
            expected_output = output
        elif output != expected_output:
            raise RuntimeError(f"{name} correctness output differs from the other runner")

    for index in range(args.runs):
        ordered = commands[index % len(commands) :] + commands[: index % len(commands)]
        for name, command in ordered:
            elapsed, rss_kib, output = run(command, args.input)
            if output != expected_output:
                raise RuntimeError(f"{name} output changed during measurement")
            samples[name].append((elapsed, rss_kib))

    for name, _ in commands:
        values = samples[name]
        times = [sample[0] for sample in values]
        rss = [sample[1] for sample in values]
        print(f"{name}: samples (seconds, max-RSS-KiB): {values}")
        print(
            f"{name}: median_seconds={statistics.median(times):.6f} "
            f"max_rss_kib={max(rss)}"
        )
    c_times = [sample[0] for sample in samples["C"]]
    tsn_times = [sample[0] for sample in samples["tsn"]]
    c_median = statistics.median(c_times)
    tsn_median = statistics.median(tsn_times)
    c_stddev = statistics.stdev(c_times) if args.runs > 1 else 0.0
    c_rss = max(sample[1] for sample in samples["C"])
    tsn_rss = max(sample[1] for sample in samples["tsn"])
    print(
        f"C_median_seconds={c_median:.6f} C_sample_stddev_seconds={c_stddev:.6f} "
        f"tsn_median_seconds={tsn_median:.6f} "
        f"tsn_median_over_c={tsn_median / c_median:.3f}x "
        f"tsn_to_c_rss_ratio={tsn_rss / max(c_rss, 1):.3f}x"
    )
    within_one_sigma = tsn_median <= c_median + c_stddev
    rss_gate = tsn_rss <= 1.2 * c_rss
    print(f"M1 throughput gate: {'PASS' if within_one_sigma else 'FAIL'}")
    print(f"M1 RSS gate: {'PASS' if rss_gate else 'FAIL'}")
    if args.raw_out:
        cpu_model = "unknown"
        cpuinfo = Path("/proc/cpuinfo")
        if cpuinfo.exists():
            for line in cpuinfo.read_text(errors="replace").splitlines():
                if line.lower().startswith("model name") and ":" in line:
                    cpu_model = line.split(":", 1)[1].strip()
                    break
        raw = {
            "platform": platform.platform(),
            "cpu_model": cpu_model,
            "input_bytes": args.input.stat().st_size,
            "input_sha256": hashlib.sha256(args.input.read_bytes()).hexdigest(),
            "runs": args.runs,
            "commands": dict(commands),
            "expected_stdout": expected_output,
            "samples_seconds_rss_kib": samples,
            "summary": {
                "c_median_seconds": c_median,
                "c_sample_stddev_seconds": c_stddev,
                "tsn_median_seconds": tsn_median,
                "c_max_rss_kib": c_rss,
                "tsn_max_rss_kib": tsn_rss,
                "throughput_gate_pass": within_one_sigma,
                "rss_gate_pass": rss_gate,
            },
        }
        args.raw_out.parent.mkdir(parents=True, exist_ok=True)
        args.raw_out.write_text(json.dumps(raw, indent=2) + "\n")
    if not (within_one_sigma and rss_gate):
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
