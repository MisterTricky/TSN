import { printU32 } from "tsn:io";
import { fnv1a } from "../stdlib/bytes.ts";

function main(): i32 {
  const bytes: Uint8Array = new Uint8Array(1);
  bytes[0] = 65;
  printU32(fnv1a(bytes));
  return 0;
}
