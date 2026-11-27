import { printI32 } from "tsn:io";

function main(): i32 {
  const takeLeft: bool = true;
  const value: i32 = takeLeft ? 7 : 9;
  printI32(value);
  return 0;
}
