import { instantiate as instantiateGet } from "./get/component.js";
import getCore1 from "./get/component.core.wasm";
import getCore2 from "./get/component.core2.wasm";
import getCore3 from "./get/component.core3.wasm";
import getCore4 from "./get/component.core4.wasm";
import { instantiate as instantiatePut } from "./put/component.js";
import putCore1 from "./put/component.core.wasm";
import putCore2 from "./put/component.core2.wasm";
import putCore3 from "./put/component.core3.wasm";
import putCore4 from "./put/component.core4.wasm";
import { instantiate as instantiateCas } from "./cas/component.js";
import casCore1 from "./cas/component.core.wasm";
import casCore2 from "./cas/component.core2.wasm";
import casCore3 from "./cas/component.core3.wasm";
import casCore4 from "./cas/component.core4.wasm";
import * as capability from "./provider-capability.mjs";
import * as objectStore from "./provider-object-store.mjs";

function instantiate(factory, cores) {
  const modules = new Map([
    ["component.core.wasm", cores[0]],
    ["component.core2.wasm", cores[1]],
    ["component.core3.wasm", cores[2]],
    ["component.core4.wasm", cores[3]],
  ]);
  return factory(
    (name) => modules.get(name),
    {
      "aiueos:capability/capability": capability,
      "aiueos:capability/object-store": objectStore,
    },
  );
}

const get = instantiate(instantiateGet, [getCore1, getCore2, getCore3, getCore4]);
const put = instantiate(instantiatePut, [putCore1, putCore2, putCore3, putCore4]);
const cas = instantiate(instantiateCas, [casCore1, casCore2, casCore3, casCore4]);

export default {
  fetch() {
    const values = [get.main(), put.main(), cas.main()];
    if (values[0] !== 4n || values[1] !== 0n || values[2] !== 1n) {
      return new Response(`FAIL:${values.join(",")}`, { status: 500 });
    }
    return new Response("PASS:compiler-component-jco-workerd:object-store:4,0,1");
  },
};
