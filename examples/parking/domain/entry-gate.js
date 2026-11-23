// The logic every entry gate shares: a barrier, a ticket printer and a FREE/FULL display.
// A gate never decides anything: it asks the controller for a space, and only the answer
// "ticket granted" prints a ticket and opens the barrier.

import { ENDPOINT, MSG, DEFAULTS } from './protocol.js';
import { createBarrier } from './barrier.js';

export function createEntryGate({ name, bus, requestTimeout = DEFAULTS.requestTimeout }) {
  const barrier = createBarrier({
    onChange: (from, to) => bus.emit(name, 'state', { machine: 'Barrier', from, to }),
  });
  const printer = { printed: [], last: null };
  const display = { free: null, capacity: null };
  let pending = null; // the car waiting for an answer from the controller
  let outcome = null; // the answer, waiting for the car to collect it

  bus.register(name, receive);

  function carArrived(car) {
    if (pending || outcome) return false;
    pending = { car, age: 0 };
    bus.send(name, ENDPOINT.controller, { type: MSG.REQUEST_SPACE, car });
    return true;
  }

  function onTicketGranted(message) {
    if (!pending || pending.car !== message.car) return;
    pending = null;
    printer.printed.push(message.ticketId); // print first ...
    printer.last = message.ticketId;
    outcome = { car: message.car, ticketId: message.ticketId };
    barrier.open(); // ... and only then raise the arm
  }

  function receive(message) {
    if (message.type === MSG.TICKET_GRANTED) onTicketGranted(message);
    else if (message.type === MSG.GARAGE_FULL) {
      if (pending && pending.car === message.car) {
        pending = null;
        outcome = { car: message.car, full: true }; // no ticket, the barrier stays down
      }
    } else if (message.type === MSG.FREE_COUNT) {
      display.free = message.free;
      display.capacity = message.capacity;
    }
  }

  function tick(dt) {
    barrier.tick(dt);
    if (pending) {
      pending.age += dt;
      if (pending.age > requestTimeout) {
        outcome = { car: pending.car, failed: true }; // no answer: no ticket, the barrier stays down
        pending = null;
        bus.emit(name, 'timeout', {});
      }
    }
  }

  function takeOutcome(car) {
    if (!outcome || outcome.car !== car) return null;
    const result = outcome;
    outcome = null;
    return result;
  }

  return {
    name,
    barrier,
    printer,
    display,
    carArrived,
    takeOutcome,
    tick,
    get busy() { return pending !== null || outcome !== null; },
    get text() {
      if (display.free === null) return '--';
      return display.free === 0 ? 'FULL' : `${display.free}`;
    },
  };
}
