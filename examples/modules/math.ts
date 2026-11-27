import { multiplyByTwo as scale } from "./common.ts";

export function double(value: u32): u32 {
  return scale(value);
}
