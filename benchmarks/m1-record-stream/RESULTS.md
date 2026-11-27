# M1 record-stream measurements

## Correctness

The 100,000,000-byte generated input was run through the C and standalone tsn
programs. Both produced:

```text
100000
100000000
1155155397
```

The C/tsn `run.py` harness compared the full output successfully. The Bun
reference was also run against the same generated input using the Windows Bun
installation and produced the same three lines. No cross-OS Bun timing is used;
Bun performance remains informational and is not part of the C-parity gate.

## Preliminary throughput and RSS

Ten measured process runs used a copy of the input on WSL's Linux `/tmp` to avoid
Windows-mounted-file I/O dominating the native compute comparison. C and tsn
were both compiled with the same Clang `-O3 -march=native -fwrapv` IR pass and
`-O0 -march=native -fwrapv -static` final link. The `measure.py` harness used
`perf_counter` for elapsed time and GNU `/usr/bin/time` for peak RSS.

| Runner | Median elapsed | Maximum peak RSS |
|---|---:|---:|
| C | 0.213570 s | 612 KiB |
| tsn | 0.201381 s | 548 KiB |
| Bun (informational) | 2.099188 s | 58,940 KiB |

C sample standard deviation was 0.018137 s. The tsn median was 0.012189 s
faster than the C median (about 1.06× C throughput), within one C standard
deviation. Peak RSS was 0.895× C. Thus the throughput and RSS portions of the
revised M1 gate pass for this measured workload. Raw samples, exact relative
commands, output, input hash, and environment are recorded in
`RESULTS-RAW.json`; `run-acceptance-linux.sh` reproduces the full workflow.

## Leak check

The 10-million-iteration allocation/cleanup fixture produced the expected
checksum `15000000`. Valgrind is not usable in this WSL image because its
required loader debug symbols are unavailable, so the same generated C was
rebuilt with Clang AddressSanitizer and run with LeakSanitizer enabled:

```sh
clang -O1 -g -fsanitize=address -fno-omit-frame-pointer \
  target/m1-leak/buffer-cleanup.c -o target/m1-leak/buffer-cleanup-asan
ASAN_OPTIONS=detect_leaks=1:exitcode=99 target/m1-leak/buffer-cleanup-asan
```

It exited successfully with no leak report. The 10M loop uses the same compiler
buffer allocator and emitted cleanup paths as the standalone build. Combined
with matching 100 MB C/Bun/tsn output, the throughput tolerance, and RSS bound,
the revised M1 gate passes for this workload.
