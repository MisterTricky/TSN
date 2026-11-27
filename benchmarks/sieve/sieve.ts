// Sieve of Eratosthenes using a byte-per-candidate mutable buffer.
export function countPrimes(limit: u32): u32 {
  const composite: Uint8Array = new Uint8Array(limit + 1);
  composite[0] = 1;
  composite[1] = 1;

  for (let p: u32 = 2; p * p <= limit; p++) {
    if (composite[p] === 0) {
      for (let multiple: u32 = p * p; multiple <= limit; multiple = multiple + p) {
        composite[multiple] = 1;
      }
    }
  }

  let count: u32 = 0;
  for (let candidate: u32 = 2; candidate <= limit; candidate++) {
    if (composite[candidate] === 0) count++;
  }
  return count;
}

export function main(): i32 {
  const start: f64 = clockMs();
  let checksum: u32 = 0;
  for (let run: u32 = 0; run < 5; run++) {
    checksum = checksum + countPrimes(1_000_000 + run);
  }
  const elapsedMs: f64 = clockMs() - start;
  printBenchmark(checksum, elapsedMs);
  return 0;
}
