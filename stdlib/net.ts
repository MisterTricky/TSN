import { writeTcpString } from "tsn:net";

// Source-level retry loop over the primitive TCP write operation.
export function writeStringAll(fd: i32, value: string): bool {
  let offset: u32 = 0;
  while (offset < value.length) {
    const count: u32 = writeTcpString(fd, value, offset);
    if (count === 0 || count === 4294967295) return false;
    offset = offset + count;
  }
  return true;
}
