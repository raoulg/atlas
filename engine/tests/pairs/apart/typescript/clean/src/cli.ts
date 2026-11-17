#!/usr/bin/env -S npx tsx
// The program sets its ledger up on the folder it keeps its data in, and flushes it unstamped.

import { Ledger } from "./store.js";

class Pipeline {
  ledger: Ledger;

  constructor() {
    this.ledger = new Ledger("data/blocked");
  }

  run(): void {
    this.ledger.flush();
  }
}

function main(): void {
  new Pipeline().run();
}

main();
