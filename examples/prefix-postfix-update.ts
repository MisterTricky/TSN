import { printI32 } from "tsn:io";

function main(): i32 {
  let value: i32 = 4;
  const postIncrement: i32 = value++;
  const preIncrement: i32 = ++value;
  const postDecrement: i32 = value--;
  const preDecrement: i32 = --value;
  printI32(postIncrement);
  printI32(preIncrement);
  printI32(postDecrement);
  printI32(preDecrement);
  printI32(value);
  return 0;
}
