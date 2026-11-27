# M3 HTTP smoke-soak measurements

The source-level keep-alive fixture and Bun reference were each run for 10
minutes on the same WSL2 host, using four load workers, persistent connections
(up to 100 requests each), and `GET /` returning the same 11-byte JSON body.
Raw commands and measurements are in `RESULTS-RAW.json`.

| Runner | Requests | Throughput | p50 | p99 | Peak RSS |
|---|---:|---:|---:|---:|---:|
| tsn | 6,129,355 | 10,215.6 req/s | 0.352 ms | 0.957 ms | 888 KiB |
| Bun | 6,867,420 | 11,445.7 req/s | 0.301 ms | 0.944 ms | 40,648 KiB |

Both runs reported zero request errors and exited cleanly after SIGTERM. The tsn
rate exceeded the 1,000 req/s target, and its p99 was 1.014x Bun's p99, within
the 2x threshold for this measured fixture.

This is evidence for the constrained fixture only, not general HTTP/1.1
conformance or M3 completion. In particular, the load harness sends SIGTERM
after its timed workers stop, so it verifies signal-aware clean exit but does
not exercise a deliberately in-flight request during shutdown. The fixed
parser rejects Transfer-Encoding and is not a complete HTTP implementation.
