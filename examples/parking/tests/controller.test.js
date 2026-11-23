import test from 'node:test';
import assert from 'node:assert/strict';
import { actors } from './helpers.js';
import { ENDPOINT, MSG, DEFAULTS } from '../domain/protocol.js';

function enter(a, gate, car) {
  assert.ok(gate.carArrived(car));
  a.bus.deliver();
  return gate.takeOutcome(car);
}

test('the displays follow the controller counter', () => {
  const a = actors({ capacity: 3 });
  assert.equal(a.entry1.display.free, 3);
  enter(a, a.entry1, 'a');
  assert.equal(a.entry2.display.free, 2);
  assert.equal(a.entry2.text, '2');
});

test('free spaces never go negative: the entries show FULL and issue no ticket', () => {
  const a = actors({ capacity: 2 });
  assert.ok(enter(a, a.entry1, 'a').ticketId);
  assert.ok(enter(a, a.entry2, 'b').ticketId);
  a.run(20); // both arms are back down
  const refused = enter(a, a.entry1, 'c');
  assert.equal(refused.full, true);
  assert.equal(refused.ticketId, undefined);
  assert.equal(a.controller.free(), 0);
  assert.equal(a.entry1.text, 'FULL');
  assert.equal(a.entry2.text, 'FULL');
  assert.equal(a.entry1.barrier.state, 'closed', 'a full garage does not raise the arm');
  assert.equal(a.entry1.printer.printed.length, 1);
});

test('two entries asking at the same instant never take the same last space', () => {
  const a = actors({ capacity: 1 });
  a.bus.setLatency(2);
  a.entry1.carArrived('a');
  a.entry2.carArrived('b');
  a.run(5);
  const one = a.entry1.takeOutcome('a');
  const two = a.entry2.takeOutcome('b');
  const granted = [one, two].filter((r) => r && r.ticketId);
  const refused = [one, two].filter((r) => r && r.full);
  assert.equal(granted.length, 1);
  assert.equal(refused.length, 1);
  assert.equal(a.controller.free(), 0);
  assert.equal(a.controller.ticketBook().length, 1);
});

test('an entry barrier opens only after the controller issued a ticket', () => {
  const a = actors({ capacity: 2 });
  a.bus.setLatency(1);
  a.entry1.carArrived('a');
  a.run(0.5);
  assert.equal(a.entry1.barrier.state, 'closed', 'asked but not answered yet');
  assert.equal(a.entry1.printer.printed.length, 0);
  a.run(3);
  assert.equal(a.entry1.printer.printed.length, 1);
  assert.notEqual(a.entry1.barrier.state, 'closed');
});

test('with the controller offline there is no ticket and the barrier stays down', () => {
  const a = actors({ capacity: 2 });
  a.bus.setOffline(ENDPOINT.controller, true);
  a.entry1.carArrived('a');
  a.run(DEFAULTS.requestTimeout + 2);
  const result = a.entry1.takeOutcome('a');
  assert.equal(result.failed, true);
  assert.equal(a.entry1.barrier.state, 'closed');
  assert.equal(a.controller.free(), 2);
});

test('payment is recorded before the exit may open; unpaid tickets are refused', () => {
  const a = actors();
  const { ticketId } = enter(a, a.entry1, 'a');
  a.exit.readTicket('a', ticketId);
  a.bus.deliver();
  assert.equal(a.exit.takeOutcome('a').denied, 'unpaid');
  assert.equal(a.exit.barrier.state, 'closed');
  assert.equal(a.controller.ticketBook()[0].state, 'issued');

  a.kiosk.insertTicket('a', ticketId);
  a.bus.deliver();
  assert.equal(a.kiosk.takeOutcome('a').ok, true);
  const ticket = a.controller.ticketBook()[0];
  assert.equal(ticket.state, 'paid');
  assert.equal(ticket.paidTotal, DEFAULTS.ratePerBlock);

  a.exit.readTicket('a', ticketId);
  a.bus.deliver();
  assert.equal(a.exit.takeOutcome('a').ok, true);
  assert.equal(a.exit.barrier.state, 'opening');
});

test('a ticket is used for exit exactly once, and frees exactly one space', () => {
  const a = actors({ capacity: 2 });
  const { ticketId } = enter(a, a.entry1, 'a');
  a.kiosk.insertTicket('a', ticketId);
  a.bus.deliver();
  a.kiosk.takeOutcome('a');
  assert.equal(a.controller.free(), 1);
  a.exit.readTicket('a', ticketId);
  a.bus.deliver();
  assert.ok(a.exit.takeOutcome('a').ok);
  assert.equal(a.controller.free(), 2);
  a.run(15);
  a.exit.readTicket('b', ticketId);
  a.bus.deliver();
  assert.equal(a.exit.takeOutcome('b').denied, 'used');
  assert.equal(a.controller.free(), 2, 'the counter never exceeds the capacity');
  a.kiosk.insertTicket('b', ticketId);
  a.bus.deliver();
  assert.equal(a.kiosk.takeOutcome('b').ok, false);
});

test('a paid ticket expires after the grace period and must be paid again', () => {
  const a = actors();
  const { ticketId } = enter(a, a.entry1, 'a');
  a.kiosk.insertTicket('a', ticketId);
  a.bus.deliver();
  a.kiosk.takeOutcome('a');
  a.run(DEFAULTS.graceSeconds - 5);
  assert.equal(a.controller.ticketBook()[0].state, 'paid');
  a.run(10);
  assert.equal(a.controller.ticketBook()[0].state, 'expired');

  a.exit.readTicket('a', ticketId);
  a.bus.deliver();
  assert.equal(a.exit.takeOutcome('a').denied, 'expired');
  assert.equal(a.exit.barrier.state, 'closed');

  a.kiosk.insertTicket('a', ticketId);
  a.bus.deliver();
  const again = a.kiosk.takeOutcome('a');
  assert.equal(again.ok, true);
  assert.ok(again.amount > 0, 'the time since the first payment is charged');
  a.exit.readTicket('a', ticketId);
  a.bus.deliver();
  assert.ok(a.exit.takeOutcome('a').ok);
});

test('unknown tickets are refused everywhere', () => {
  const a = actors();
  a.exit.readTicket('x', 'T999');
  a.bus.deliver();
  assert.equal(a.exit.takeOutcome('x').denied, 'unknown');
  a.kiosk.insertTicket('x', 'T999');
  a.bus.deliver();
  assert.equal(a.kiosk.takeOutcome('x').reason, 'unknown');
});

test('actors reach each other only through the bus', () => {
  const a = actors();
  enter(a, a.entry1, 'a');
  const sends = a.events.filter((e) => e.kind === 'send');
  assert.ok(sends.some((e) => e.entity === ENDPOINT.entry1 && e.to === ENDPOINT.controller && e.type === MSG.REQUEST_SPACE));
  assert.ok(sends.some((e) => e.entity === ENDPOINT.controller && e.to === ENDPOINT.entry1 && e.type === MSG.TICKET_GRANTED));
});
