#!/usr/bin/env python3
"""Generate the M1 input and compare stdout from supplied reference commands."""
import argparse
import shlex
import subprocess
import sys
from pathlib import Path

RECORDS = 100_000
PAYLOAD_BYTES = 996
RECORD_BYTES = 1_000
TOTAL_BYTES = RECORDS * RECORD_BYTES


def generate(path: Path) -> None:
    with path.open("wb") as output:
        for record in range(RECORDS):
            output.write(PAYLOAD_BYTES.to_bytes(4, "little"))
            output.write(bytes((record * 17 + i * 31 + 0x5A) & 0xFF for i in range(PAYLOAD_BYTES)))
    if path.stat().st_size != TOTAL_BYTES:
        raise RuntimeError("generated input has unexpected size")


def run(label: str, command: str, input_path: Path) -> str:
    with input_path.open("rb") as source:
        result = subprocess.run(shlex.split(command), stdin=source, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, check=False)
    if result.returncode:
        raise RuntimeError(f"{label} exited {result.returncode}: {result.stderr.decode(errors='replace')}")
    return result.stdout.decode("ascii").strip()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, default=Path(__file__).with_name("input.bin"))
    parser.add_argument("--c", required=True, help='C runner command, e.g. "./record-stream-c"')
    parser.add_argument("--bun", help='optional Bun runner command, e.g. "bun run record-stream.bun.ts"')
    parser.add_argument("--tsn", help="optional compiled tsn executable command")
    args = parser.parse_args()
    generate(args.input)
    commands = [("C", args.c)]
    if args.bun:
        commands.append(("Bun", args.bun))
    if args.tsn:
        commands.append(("tsn", args.tsn))
    if len(commands) < 2:
        parser.error("provide at least one of --bun or --tsn in addition to --c")
    outputs = [(label, run(label, command, args.input)) for label, command in commands]
    for label, output in outputs:
        print(f"{label}: {output}")
    if any(output != outputs[0][1] for _, output in outputs[1:]):
        print("output mismatch", file=sys.stderr)
        return 1
    print("outputs match")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
