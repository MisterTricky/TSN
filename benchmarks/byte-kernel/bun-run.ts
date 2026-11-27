import { kernel } from "./kernel.ts";

const n = 16_777_216;
// Warm the imported function, then time a batch to reduce clock/scheduler noise.
for (let i = 0; i < 3; i++) kernel(n);
const start = performance.now();
let checksum = 0;
for (let i = 0; i < 5; i++) checksum ^= kernel(n + i);
const elapsedMs = performance.now() - start;
console.log(JSON.stringify({ checksum, elapsedMs }));
