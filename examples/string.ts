import { printString } from "tsn:io";

function main(): i32 {
  const greeting: string = "héllo, tsn";
  printString(greeting);
  if (greeting.length === 11 && greeting === "héllo, tsn") return 0;
  return 1;
}
