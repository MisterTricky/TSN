#!/usr/bin/env python3
"""Run bounded concurrent HTTP/1.1 keep-alive load and report p99/RSS."""

import argparse
import shlex
import socket
import subprocess
import threading
import time
from pathlib import Path

REQUEST = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n"
BODY = b'{"ok":true}'


def read_response(stream) -> None:
    status = stream.readline().rstrip(b"\r\n")
    if status != b"HTTP/1.1 200 OK":
        raise RuntimeError(f"unexpected HTTP status line: {status!r}")
    headers = {}
    while True:
        line = stream.readline()
        if not line:
            raise RuntimeError("server closed in response headers")
        if line in (b"\r\n", b"\n"):
            break
        name, separator, value = line.partition(b":")
        if not separator:
            raise RuntimeError(f"malformed response header: {line!r}")
        headers[name.strip().lower()] = value.strip().lower()
    if headers.get(b"content-length") != b"11":
        raise RuntimeError(f"unexpected Content-Length: {headers.get(b'content-length')!r}")
    if stream.read(11) != BODY:
        raise RuntimeError("unexpected HTTP body")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", required=True, help="quoted server command")
    parser.add_argument("--duration-seconds", type=float, default=10)
    parser.add_argument("--workers", type=int, default=4)
    parser.add_argument("--port", type=int, default=24875)
    parser.add_argument("--requests-per-connection", type=int, default=100)
    args = parser.parse_args()
    if args.duration_seconds <= 0 or args.workers < 1 or args.requests_per_connection < 1:
        parser.error("duration, workers, and requests per connection must be positive")

    process = subprocess.Popen(shlex.split(args.server))
    stop = threading.Event()
    lock = threading.Lock()
    latencies: list[float] = []
    completed = 0
    failures = 0
    failure_messages: list[str] = []
    peak_rss_kib = 0

    ready_deadline = time.monotonic() + 10
    while True:
        try:
            with socket.create_connection(("127.0.0.1", args.port), timeout=1) as client:
                client.settimeout(2)
                client.sendall(REQUEST)
                with client.makefile("rb") as response:
                    read_response(response)
            break
        except OSError:
            if process.poll() is not None:
                raise RuntimeError(f"server exited before readiness: {process.returncode}")
            if time.monotonic() >= ready_deadline:
                raise RuntimeError("server did not become ready")
            time.sleep(0.01)

    def sample_rss() -> None:
        nonlocal peak_rss_kib
        status = Path(f"/proc/{process.pid}/status")
        while not stop.wait(0.1):
            try:
                for line in status.read_text().splitlines():
                    if line.startswith("VmHWM:"):
                        with lock:
                            peak_rss_kib = max(peak_rss_kib, int(line.split()[1]))
                        break
            except OSError:
                return

    def worker() -> None:
        nonlocal completed, failures
        deadline = time.monotonic() + args.duration_seconds
        while not stop.is_set() and time.monotonic() < deadline:
            try:
                client = socket.create_connection(("127.0.0.1", args.port), timeout=2)
                client.settimeout(2)
                with client, client.makefile("rb") as response:
                    for _ in range(args.requests_per_connection):
                        if stop.is_set() or time.monotonic() >= deadline:
                            break
                        started = time.perf_counter()
                        client.sendall(REQUEST)
                        read_response(response)
                        latency_ms = (time.perf_counter() - started) * 1000.0
                        with lock:
                            completed += 1
                            latencies.append(latency_ms)
            except (OSError, RuntimeError) as error:
                with lock:
                    failures += 1
                    if len(failure_messages) < 5:
                        failure_messages.append(str(error))
                if process.poll() is not None:
                    return

    rss_thread = threading.Thread(target=sample_rss, daemon=True)
    rss_thread.start()
    started = time.monotonic()
    workers = [threading.Thread(target=worker) for _ in range(args.workers)]
    for thread in workers:
        thread.start()
    for thread in workers:
        thread.join(args.duration_seconds + 10)
    elapsed = time.monotonic() - started
    stop.set()
    rss_thread.join(timeout=1)

    if any(thread.is_alive() for thread in workers):
        process.terminate()
        raise RuntimeError("load workers did not stop")
    process.send_signal(15)  # SIGTERM; tsn should leave its accept loop cleanly.
    status = process.wait(timeout=5)
    if status != 0:
        raise RuntimeError(f"server exited with {status} after SIGTERM")
    if not latencies:
        raise RuntimeError("no successful responses")

    ordered = sorted(latencies)
    p50 = ordered[(len(ordered) - 1) * 50 // 100]
    p99 = ordered[(len(ordered) - 1) * 99 // 100]
    print(
        f"requests={completed} elapsed_seconds={elapsed:.3f} "
        f"rps={completed / elapsed:.1f} p50_ms={p50:.3f} p99_ms={p99:.3f} "
        f"errors={failures} peak_rss_kib={peak_rss_kib} error_samples={failure_messages}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
