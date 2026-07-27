import { Grant } from "./provider-capability.mjs";

let calls = 0;

export function now(authority) {
  if (!(authority instanceof Grant) || authority.operation !== "clock/now") {
    throw "provider-failed";
  }
  calls += 1;
  if (calls > 1) throw "quota";
  return 4242n;
}
