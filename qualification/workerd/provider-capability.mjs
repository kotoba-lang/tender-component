const admittedOperation = "clock/now";
let issued = false;

export class Grant {
  constructor(operation) {
    if (operation !== admittedOperation || issued) throw "quota";
    this.operation = operation;
    issued = true;
  }
}

export function acquire(request) {
  if (request !== "clock-now") throw "provider-failed";
  return new Grant(admittedOperation);
}
