#!/usr/bin/env python3
"""Exercise the blocking tsn:net primitives against a one-shot TCP program."""

import socket
import subprocess
import sys
import time


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: tcp-smoke-linux.py <server executable>")
    process = subprocess.Popen([sys.argv[1]])
    try:
        deadline = time.monotonic() + 10
        while True:
            try:
                with socket.create_connection(("127.0.0.1", 24873), timeout=1) as client:
                    client.settimeout(3)
                    client.sendall(b"PING")
                    response = client.recv(4)
                if response != b"PONG":
                    raise RuntimeError(f"unexpected TCP response: {response!r}")
                break
            except OSError:
                if process.poll() is not None:
                    raise RuntimeError(f"TCP program exited early: {process.returncode}")
                if time.monotonic() >= deadline:
                    raise RuntimeError("TCP server did not accept a connection")
                time.sleep(0.01)
        status = process.wait(timeout=3)
        if status != 0:
            raise RuntimeError(f"TCP program exited with {status}")
        print("TCP smoke test passed")
        return 0
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=3)


if __name__ == "__main__":
    raise SystemExit(main())
