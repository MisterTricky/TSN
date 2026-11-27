import { acceptTcp, closeTcp, listenTcp, readTcp, writeTcpString } from "tsn:net";
import { isEmptyGetRootRequest } from "../stdlib/http.ts";

function main(): i32 {
  const listener: i32 = listenTcp(24874, true);
  if (listener < 0) return 1;

  const connection: i32 = acceptTcp(listener);
  if (connection < 0) {
    closeTcp(listener);
    return 2;
  }

  const request: Uint8Array = new Uint8Array(18);
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

  const response: string = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK";
  let sent: u32 = 0;
  while (sent < response.length) {
    const count: u32 = writeTcpString(connection, response, sent);
    if (count === 0 || count === 4294967295) {
      closeTcp(connection);
      closeTcp(listener);
      return 5;
    }
    sent = sent + count;
  }

  closeTcp(connection);
  closeTcp(listener);
  return 0;
}
