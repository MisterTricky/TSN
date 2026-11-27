# M5 realistic-service fixture

This directory is a self-contained, deterministic service-shaped workload for
future M5 comparisons. `service.tsn.ts` is an early source-language core with
matching workload checksum and a small fixed-schema JSON parser/response builder.
It is not yet an HTTP server or a complete generic JSON package. This is a
fixture, **not** a production authentication implementation and not evidence
that M5 passes. It makes no performance claim.
It has no package or network dependency: the Bun reference uses Bun's Node
compatibility implementation of `node:crypto` (`createHmac` and
`timingSafeEqual`). Use a Bun release that provides these APIs.

## Run

From this directory, with Bun installed:

```sh
bun run run.ts             # correctness checks, then 100,000 requests
bun run run.ts 1000000     # same workload, chosen iteration count
```

The script checks correctness before timing. It prints a checksum and elapsed
milliseconds, but those values are machine/runtime-specific and are not saved
as claimed results. Keep compiler build time outside the timed loop.

## Work performed per request

`service.ts` parses a three-segment JWT-like token; validates canonical
base64url encoding, JSON header, `alg: HS256`, signature length and HMAC-SHA256;
compares the signature with Node-compatible `timingSafeEqual`; parses and
validates claims (`sub`, future `exp`, fixed `iss`); checks a small request
schema; and constructs a templated response. Malformed tokens return the same
401 response, invalid request bodies return 400, and accepted requests return
200 with a deterministic total. There is no network server, I/O, clock lookup,
randomness, or external state in the hot path. The caller supplies fixed `now`.

The fixture secret is intentionally public and unsuitable for real use. For a
production implementation use a vetted cryptographic library/API with a
documented constant-time MAC verification primitive; do not replace this with
ordinary string equality. The Bun reference uses `timingSafeEqual` only after
checking equal lengths.

## Matching a future tsn implementation

For an apples-to-apples comparison, the future implementation must:

1. Run on the same OS, machine, and benchmark conditions as Bun; record exact
   runtime/compiler versions and commands. Do not compare Windows and Linux
   artifacts or include compilation/link time in request throughput.
2. Consume the same five `workload` entries in `payloads.ts`, in the same order
   repeated modulo five, with identical token bytes, request objects, fixed
   time (`1_900_000_000`), and iteration count.
3. Perform the same token parsing, header/claim/schema validation, HMAC-SHA256
   verification with constant-time signature comparison, and response
   construction. Do not substitute a prevalidated token, skip crypto, cache
   validation, or benchmark a reduced operation.
4. Match every correctness case and response status/body behavior in `run.ts`;
   report the same checksum for a given iteration count. Keep warmup policy,
   measurement repetitions, and aggregation identical when adding comparative
   results. Document any unavoidable API/implementation differences.
5. Report raw measurements and environment before drawing conclusions. This
   fixture defines a workload only; it does not establish M5 support or a pass.

## Files

- `service.ts`: reference endpoint, token generator, and deterministic logic.
- `payloads.ts`: fixed valid and invalid benchmark inputs.
- `run.ts`: correctness cases followed by the repeated benchmark workload.
- `service.tsn.ts`: early tsn source implementation of the fixture core; compile
  with `tsn compile benchmarks/service-m5/service.tsn.ts -o target/m5-service`.
