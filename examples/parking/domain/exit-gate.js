// The Exit Gate: a barrier that reads tickets. The barrier opens only when the controller
// answers "exit granted", which it does only for a paid ticket that has not expired.

import { ENDPOINT, MSG, DEFAULTS } from './protocol.js';
import { createBarrier } from './barrier.js';

const MESSAGES = {
  unpaid: 'PAY FIRST',
  expired: 'EXPIRED: PAY AGAIN',
  used: 'TICKET USED',
  unknown: 'UNKNOWN TICKET',
};

export function createExitGate({ bus, requestTimeout = DEFAULTS.requestTimeout }) {
  const name = ENDPOINT.exit;
  const barrier = createBarrier({
    onChange: (from, to) => bus.emit(name, 'state', { machine: 'Barrier', from, to }),
  });
  const screen = { text: 'INSERT TICKET' };
  let pending = null;
  let outcome = null;

  bus.register(name, receive);

  function readTicket(car, ticketId) {
    if (pending || outcome) return false;
    pending = { car, age: 0 };
    screen.text = 'CHECKING...';
    bus.send(name, ENDPOINT.controller, { type: MSG.EXIT_REQUEST, car, ticketId });
    return true;
  }

  function receive(message) {
    if (!pending || pending.car !== message.car) return;
    if (message.type === MSG.EXIT_GRANTED) {
      pending = null;
      screen.text = 'THANK YOU';
      outcome = { car: message.car, ok: true };
      barrier.open();
    } else if (message.type === MSG.EXIT_DENIED) {
      pending = null;
      screen.text = MESSAGES[message.reason] || 'DENIED';
      outcome = { car: message.car, denied: message.reason };
    }
  }

  function tick(dt) {
    barrier.tick(dt);
    if (pending) {
      pending.age += dt;
      if (pending.age > requestTimeout) {
        outcome = { car: pending.car, denied: 'no contact' };
        screen.text = 'NO CONTACT';
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

  function reset() {
    if (!pending && !outcome) screen.text = 'INSERT TICKET';
  }

  return {
    name,
    barrier,
    screen,
    readTicket,
    takeOutcome,
    tick,
    reset,
    get busy() { return pending !== null || outcome !== null; },
  };
}
