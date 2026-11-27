import { handleRequest, makeToken } from "./service";
import { token, validRequest, workload } from "./payloads";

const cases = [
  { name: "valid request", token, body: validRequest, status: 200, total: 375 },
  { name: "bad signature", token: `${token.slice(0, -1)}x`, body: validRequest, status: 401 },
  { name: "malformed token", token: "a.b", body: validRequest, status: 401 },
  { name: "expired token", token: makeToken({ sub: "user-0001", exp: 1, iss: "m5-fixture" }), body: validRequest, status: 401 },
  { name: "invalid schema", token, body: { requestId: "BAD ID", item: { name: "widget", quantity: 3 } }, status: 400 },
];
for (const c of cases) {
  const actual = handleRequest(c.token, c.body);
  if (actual.status !== c.status || ("total" in c && actual.body.total !== c.total)) throw new Error(`failed: ${c.name}`);
}
console.log(`Correctness: ${cases.length} cases passed`);

const iterations = Number(Bun.argv[2] ?? 100_000);
if (!Number.isSafeInteger(iterations) || iterations < 1) throw new Error("iterations must be a positive integer");
let checksum = 0;
const start = performance.now();
for (let i = 0; i < iterations; i++) {
  const input = workload[i % workload.length];
  const response = handleRequest(input.token, input.body);
  checksum = (checksum + response.status + (response.body.total ?? 0)) >>> 0;
}
const elapsedMs = performance.now() - start;
console.log(JSON.stringify({ iterations, workloadLength: workload.length, checksum, elapsedMs: Number(elapsedMs.toFixed(3)) }));
