function main(): i32 {
  const bytes: Uint8Array = new Uint8Array(1);
  const value: u8 = bytes[1];
  printBenchmark(value, 0.0);
  return 0;
}
