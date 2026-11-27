import { countPrimes } from "./sieve.ts";

for (let warmup = 0; warmup < 3; warmup++) countPrimes(1_000_000);
const start = performance.now();
let checksum = 0;
for (let run = 0; run < 5; run++) checksum += countPrimes(1_000_000 + run);
const elapsedMs = performance.now() - start;
console.log(JSON.stringify({ checksum, elapsedMs }));
