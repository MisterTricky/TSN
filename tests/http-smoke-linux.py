#!/usr/bin/env python3
"""Exercise the source-level HTTP smoke route over loopback TCP."""

import socket
import subprocess
import sys
import time


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: http-smoke-linux.py <server executable>")
    process = subprocess.Popen([sys.argv[1]])
    try:
        deadline = time.monotonic() + 10
        while True:
            try:
                with socket.create_connection(("127.0.0.1", 24874), timeout=1) as client:
                    client.settimeout(3)
                    client.sendall(b"GET / HTTP/1.1\r\n\r\n")
                    response = bytearray()
                    while len(response) < 59:
                        chunk = client.recv(128)
                        if not chunk:
                            break
                        response.extend(chunk)
                expected = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK"
                if bytes(response) != expected:
                    raise RuntimeError(f"unexpected HTTP response: {bytes(response)!r}")
                break
            except OSError:
                if process.poll() is not None:
                    raise RuntimeError(f"HTTP program exited early: {process.returncode}")
                if time.monotonic() >= deadline:
                    raise RuntimeError("HTTP server did not accept a connection")
                time.sleep(0.01)
        status = process.wait(timeout=3)
        if status != 0:
            raise RuntimeError(f"HTTP program exited with {status}")
        print("source-level HTTP smoke test passed")
        return 0
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=3)


if __name__ == "__main__":
    raise SystemExit(main())
