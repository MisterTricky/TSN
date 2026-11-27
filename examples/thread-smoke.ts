import { spawnThread } from "tsn:os";
import { printI32 } from "tsn:io";

function worker(value: i32): void {
  printI32(value);
}

function main(): i32 {
  const status: i32 = spawnThread(worker, 7);
  if (status !== 0) return 1;
  return 0;
}
