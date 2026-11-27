import { double as timesTwo } from "./math.ts";

function main(): i32 {
  const result: u32 = timesTwo(21);
  printBenchmark(result, 0.0);
  return 0;
}
