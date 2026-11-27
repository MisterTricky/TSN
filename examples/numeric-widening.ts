function main(): i32 {
  const count: u32 = 1234;
  const asNumber: number = count;
  const small: u16 = 123;
  const asFloat: f32 = small;
  printBenchmark(count, asNumber + asFloat);
  return 0;
}
