import { acceptTcp, closeTcp, listenTcp, readTcp } from "tsn:net";
import { isEmptyGetRootRequest } from "../stdlib/http.ts";
import { writeStringAll } from "../stdlib/net.ts";

function main(): i32 {
  const listener: i32 = listenTcp(24875, true);
  if (listener < 0) return 1;

  const keepAliveResponse: string = "HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: keep-alive\r\n\r\n{\"ok\":true}";
  const closeResponse: string = "HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}";
  const request: Uint8Array = new Uint8Array(18);
  let total: u32 = 0;

  while (total < 1000) {
    const connection: i32 = acceptTcp(listener);
    if (connection < 0) {
      closeTcp(listener);
      return 2;
    }
    let handled: u32 = 0;
    while (handled < 100 && total < 1000) {
      let received: u32 = 0;
      while (received < 18) {
        const count: u32 = readTcp(connection, request, received);
        if (count === 0 || count === 4294967295) {
          closeTcp(connection);
          closeTcp(listener);
          return 3;
        }
        received = received + count;
      }
      if (!isEmptyGetRootRequest(request, received)) {
        closeTcp(connection);
        closeTcp(listener);
        return 4;
      }

      let written: bool = false;
      if (total === 999) {
        written = writeStringAll(connection, closeResponse);
      } else {
        written = writeStringAll(connection, keepAliveResponse);
      }
      if (!written) {
          closeTcp(connection);
          closeTcp(listener);
          return 5;
      }
      handled++;
      total++;
    }
    closeTcp(connection);
  }

  closeTcp(listener);
  return 0;
}
