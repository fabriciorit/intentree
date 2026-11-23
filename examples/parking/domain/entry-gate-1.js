// Entry gate 1: its own process, with its own barrier and ticket printer.
import { ENDPOINT } from './protocol.js';
import { createEntryGate } from './entry-gate.js';

export const NAME = ENDPOINT.entry1;

export function createEntryGate1({ bus }) {
  return createEntryGate({ name: NAME, bus });
}
