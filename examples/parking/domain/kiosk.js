// The Payment Kiosk: takes payment for a ticket. It only forwards the request to the
// controller, which records the payment.

import { ENDPOINT, MSG, DEFAULTS } from './protocol.js';

export function createKiosk({ bus, requestTimeout = DEFAULTS.requestTimeout }) {
  const name = ENDPOINT.kiosk;
  const screen = { text: 'INSERT TICKET', amount: null };
  let pending = null;
  let outcome = null;

  bus.register(name, receive);

  function insertTicket(car, ticketId) {
    if (pending || outcome) return false;
    pending = { car, age: 0 };
    screen.text = 'PROCESSING...';
    screen.amount = null;
    bus.send(name, ENDPOINT.controller, { type: MSG.PAY, car, ticketId });
    return true;
  }

  function receive(message) {
    if (!pending || pending.car !== message.car) return;
    if (message.type === MSG.PAYMENT_ACCEPTED) {
      pending = null;
      screen.text = 'PAID';
      screen.amount = message.amount;
      outcome = { car: message.car, ok: true, amount: message.amount };
    } else if (message.type === MSG.PAYMENT_REJECTED) {
      pending = null;
      screen.text = `REFUSED: ${message.reason}`;
      outcome = { car: message.car, ok: false, reason: message.reason };
    }
  }

  function tick(dt) {
    if (pending) {
      pending.age += dt;
      if (pending.age > requestTimeout) {
        outcome = { car: pending.car, ok: false, reason: 'no contact' };
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
    if (!pending && !outcome) {
      screen.text = 'INSERT TICKET';
      screen.amount = null;
    }
  }

  return {
    name,
    screen,
    insertTicket,
    takeOutcome,
    tick,
    reset,
    get busy() { return pending !== null || outcome !== null; },
  };
}
