import { contains } from "../stdlib/strings.ts";

function main(): i32 {
  if (!contains("héllo, tsn", "éllo")) return 1;
  if (!contains("héllo, tsn", "")) return 2;
  if (contains("héllo, tsn", "nope")) return 3;
  return 0;
}
