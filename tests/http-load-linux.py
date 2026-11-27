#!/usr/bin/env python3
"""Send 1,000 requests over HTTP/1.1 keep-alive connections to a tsn server."""

import socket
import subprocess
import sys
import time

PORT = 24875
REQUESTS = 1000
PER_CONNECTION = 100
REQUEST = b"GET / HTTP/1.1\r\n\r\n"
BODY = b'{"ok":true}'


def recv_response(client: socket.socket) -> None:
    header = bytearray()
    while not header.endswith(b"\r\n\r\n"):
        byte = client.recv(1)
        if not byte:
            raise RuntimeError("server closed before response headers completed")
        header.extend(byte)
        if len(header) > 4096:
            raise RuntimeError("response headers exceeded limit")
    lines = header.decode("ascii").split("\r\n")
    if lines[0] != "HTTP/1.1 200 OK" or "Content-Length: 11" not in lines:
        raise RuntimeError(f"unexpected response headers: {lines!r}")
    response_body = bytearray()
    while len(response_body) < 11:
        chunk = client.recv(11 - len(response_body))
        if not chunk:
            raise RuntimeError("server closed before response body completed")
        response_body.extend(chunk)
    if bytes(response_body) != BODY:
        raise RuntimeError(f"unexpected response body: {bytes(response_body)!r}")


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: http-load-linux.py <server executable>")
    process = subprocess.Popen([sys.argv[1]])
    start = time.perf_counter()
    try:
        deadline = start + 10
        for _ in range(REQUESTS // PER_CONNECTION):
            while True:
                try:
                    client = socket.create_connection(("127.0.0.1", PORT), timeout=1)
                    break
                except OSError:
                    if process.poll() is not None:
                        raise RuntimeError(f"HTTP server exited early: {process.returncode}")
                    if time.monotonic() >= deadline:
                        raise RuntimeError("HTTP server did not start listening")
                    time.sleep(0.01)
            with client:
                client.settimeout(3)
                client.sendall(REQUEST * PER_CONNECTION)
                for _ in range(PER_CONNECTION):
                    recv_response(client)
        elapsed = time.perf_counter() - start
        status = process.wait(timeout=3)
        if status != 0:
            raise RuntimeError(f"HTTP server exited with {status}")
        rps = REQUESTS / elapsed
        print(f"requests={REQUESTS} elapsed_seconds={elapsed:.6f} rps={rps:.1f}")
        if rps < 1000:
            raise RuntimeError(f"M3 smoke throughput below 1k req/s: {rps:.1f}")
        return 0
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=3)


if __name__ == "__main__":
    raise SystemExit(main())
