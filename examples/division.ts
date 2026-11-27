import { printF64 } from "tsn:io";

function main(): i32 {
  const integerOperands: f64 = 7 / 2;
  const mixedOperands: f64 = -7.0 / 2;
  printF64(integerOperands);
  printF64(mixedOperands);
  return 0;
}
