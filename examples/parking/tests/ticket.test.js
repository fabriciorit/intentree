import test from 'node:test';
import assert from 'node:assert/strict';
import { createTicket, moveTicket, totalFee, TICKET_TRANSITIONS } from '../domain/ticket.js';

test('a ticket goes issued -> paid -> exited', () => {
  const ticket = createTicket('T1', 0, 'entry gate 1');
  assert.equal(ticket.state, 'issued');
  moveTicket(ticket, 'paid');
  moveTicket(ticket, 'exited');
  assert.equal(ticket.state, 'exited');
});

test('an unpaid ticket cannot be used for exit, and an exited ticket cannot come back', () => {
  const ticket = createTicket('T1', 0, 'entry gate 1');
  assert.throws(() => moveTicket(ticket, 'exited'));
  moveTicket(ticket, 'paid');
  moveTicket(ticket, 'exited');
  assert.throws(() => moveTicket(ticket, 'paid'));
  assert.throws(() => moveTicket(ticket, 'expired'));
});

test('a ticket that already exited stays exited when used again', () => {
  const ticket = createTicket('T1', 0, 'entry gate 1');
  moveTicket(ticket, 'paid');
  moveTicket(ticket, 'exited');
  moveTicket(ticket, 'exited');
  assert.equal(ticket.state, 'exited');
});

test('a paid ticket can expire and then be paid again', () => {
  const ticket = createTicket('T1', 0, 'entry gate 1');
  moveTicket(ticket, 'paid');
  moveTicket(ticket, 'expired');
  assert.throws(() => moveTicket(ticket, 'exited'));
  moveTicket(ticket, 'paid');
  assert.equal(ticket.state, 'paid');
  assert.equal(TICKET_TRANSITIONS.length, 5);
});

test('the fee grows by one rate for each started block', () => {
  const ticket = createTicket('T1', 100, 'entry gate 1');
  assert.equal(totalFee(ticket, 100, 900, 1.5), 1.5);
  assert.equal(totalFee(ticket, 100 + 900, 900, 1.5), 1.5);
  assert.equal(totalFee(ticket, 100 + 901, 900, 1.5), 3);
});
