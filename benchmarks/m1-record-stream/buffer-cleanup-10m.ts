import { printU32 } from "tsn:io";

function main(): i32 {
  let index: u32 = 0;
  let checksum: u32 = 0;
  while (index < 10000000) {
    {
      const bytes: Uint8Array = new Uint8Array(16);
      if (index < 5000000) {
        bytes[0] = 1;
      } else {
        bytes[0] = 2;
      }
      checksum = checksum + bytes[0];
    }
    index++;
  }
  printU32(checksum);
  return 0;
}
