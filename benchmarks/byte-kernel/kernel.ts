// This module is both compiled by tsn and imported by the Bun harness.
export function kernel(n: u32): u32 {
  let hash: u32 = 2166136261;
  let acc: u32 = 0;
  for (let i: u32 = 0; i < n; i++) {
    const byte: u32 = (i * 31 + (i >>> 7) + 17) & 255;
    hash = Math.imul(hash ^ byte, 16777619) >>> 0;
    acc = (acc + ((byte * 13 + i) & 65535)) >>> 0;
  }
  return (hash ^ acc) >>> 0;
}

// The standalone tsn program's entry point. These two functions are M1
// compiler/runtime builtins; they will become ordinary stdlib imports later.
export function main(): i32 {
  const n: u32 = 16_777_216;
  const start: f64 = clockMs();
  let checksum: u32 = 0;
  for (let run: u32 = 0; run < 5; run++) {
    checksum = checksum ^ kernel(n + run);
  }
  const elapsedMs: f64 = clockMs() - start;
  printBenchmark(checksum, elapsedMs);
  return 0;
}
