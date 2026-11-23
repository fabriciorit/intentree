// Entry gate 2: its own process, with its own barrier and ticket printer.
import { ENDPOINT } from './protocol.js';
import { createEntryGate } from './entry-gate.js';

export const NAME = ENDPOINT.entry2;

export function createEntryGate2({ bus }) {
  return createEntryGate({ name: NAME, bus });
}
