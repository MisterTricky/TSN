import { parseM5RequestBody } from "../stdlib/json.ts";
import { utf8ToBytes } from "../stdlib/strings.ts";
import { sliceUtf8 } from "tsn:strings";

function main(): i32 {
  const bodyText: string = "{\"requestId\":\"req-0001\",\"item\":{\"name\":\"widget\",\"quantity\":3}}";
  const body: Uint8Array = new Uint8Array(bodyText.length);
  utf8ToBytes(bodyText, body);
  const fields: Uint32Array = new Uint32Array(5);
  if (!parseM5RequestBody(body, body.length, fields)) return 1;
  if (fields[1] !== 8 || fields[3] !== 6 || fields[4] !== 3) return 2;
  if (sliceUtf8(bodyText, fields[0], fields[0] + fields[1]) !== "req-0001") return 3;
  if (sliceUtf8(bodyText, fields[2], fields[2] + fields[3]) !== "widget") return 4;
  return 0;
}
