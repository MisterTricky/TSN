import { readStdin } from "tsn:io";

function main(): i32 {
  const bytes: Uint8Array = new Uint8Array(4);
  const first: u32 = readStdin(bytes);
  if (first !== 4) return 1;
  if (bytes[0] !== 65 || bytes[1] !== 66 || bytes[2] !== 67 || bytes[3] !== 68) return 2;
  return 0;
}
