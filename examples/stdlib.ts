import { monotonicMs } from "tsn:time";
import { printString } from "tsn:io";

function main(): i32 {
  const started: number = monotonicMs();
  printString("tsn stdlib imports are linked");
  if (started >= 0) return 0;
  return 1;
}
