// The Controller: the central server. It alone owns the free-space counter, the ticket book and
// the time that matters (the grace period). Requests from both entries, the kiosk and the exit
// arrive in one inbox and are served one at a time, so two entries can never take the last space.

import { ENDPOINT, MSG, DEFAULTS } from './protocol.js';
import { createTicket, moveTicket, totalFee } from './ticket.js';

export function createController({
  bus,
  clock,
  capacity = DEFAULTS.capacity,
  graceSeconds = DEFAULTS.graceSeconds,
  blockSeconds = DEFAULTS.blockSeconds,
  ratePerBlock = DEFAULTS.ratePerBlock,
  subscribers = [ENDPOINT.entry1, ENDPOINT.entry2],
} = {}) {
  const name = ENDPOINT.controller;
  const book = new Map(); // the ticket book: ticket id -> ticket
  const inbox = []; // the request queue
  let freeSpaces = capacity; // the free-space counter
  let nextNumber = 1;
  let locked = false; // the counter lock: one request at a time

  bus.register(name, receive);

  function reply(to, type, payload = {}) {
    bus.send(name, to, { type, ...payload });
  }

  function setState(ticket, to) {
    const from = ticket.state;
    moveTicket(ticket, to);
    bus.emit(name, 'state', { machine: 'Ticket', ticket: ticket.id, from, to });
  }

  // A paid ticket that waited past the grace period expires; checked before every decision.
  function refresh(ticket) {
    if (ticket && ticket.state === 'paid' && clock.now() - ticket.paidAt > graceSeconds) setState(ticket, 'expired');
  }

  function announce() {
    for (const to of subscribers) reply(to, MSG.FREE_COUNT, { free: freeSpaces, capacity });
  }

  function requestSpace(from, message) {
    if (freeSpaces <= 0) {
      reply(from, MSG.GARAGE_FULL, { car: message.car });
      return;
    }
    freeSpaces -= 1;
    const id = `T${String(nextNumber++).padStart(3, '0')}`;
    const ticket = createTicket(id, clock.now(), from);
    book.set(id, ticket);
    bus.emit(name, 'state', { machine: 'Ticket', ticket: id, from: null, to: 'issued' });
    reply(from, MSG.TICKET_GRANTED, { car: message.car, ticketId: id });
    announce();
  }

  function pay(from, message) {
    const ticket = book.get(message.ticketId);
    refresh(ticket);
    if (!ticket) return reply(from, MSG.PAYMENT_REJECTED, { car: message.car, reason: 'unknown' });
    if (ticket.state === 'exited') return reply(from, MSG.PAYMENT_REJECTED, { car: message.car, reason: 'used' });
    if (ticket.state === 'paid') return reply(from, MSG.PAYMENT_REJECTED, { car: message.car, reason: 'already paid' });
    const amount = totalFee(ticket, clock.now(), blockSeconds, ratePerBlock) - ticket.paidTotal;
    ticket.paidAt = clock.now(); // payment is recorded here, before anything can open the exit
    ticket.paidTotal += amount;
    setState(ticket, 'paid');
    reply(from, MSG.PAYMENT_ACCEPTED, { car: message.car, ticketId: ticket.id, amount });
  }

  function exitRequest(from, message) {
    const ticket = book.get(message.ticketId);
    refresh(ticket);
    const deny = (reason) => reply(from, MSG.EXIT_DENIED, { car: message.car, reason });
    if (!ticket) return deny('unknown');
    if (ticket.state === 'issued') return deny('unpaid');
    if (ticket.state === 'expired') return deny('expired');
    if (ticket.state === 'exited') {
      setState(ticket, 'exited'); // refused: the ticket stays exited
      return deny('used');
    }
    ticket.exitedAt = clock.now(); // paid and in time: the ticket is used up, exactly once
    setState(ticket, 'exited');
    freeSpaces += 1;
    reply(from, MSG.EXIT_GRANTED, { car: message.car, ticketId: ticket.id });
    announce();
  }

  function handle(message, from) {
    if (message.type === MSG.REQUEST_SPACE) requestSpace(from, message);
    else if (message.type === MSG.PAY) pay(from, message);
    else if (message.type === MSG.EXIT_REQUEST) exitRequest(from, message);
  }

  function checkInvariants() {
    const open = [...book.values()].filter((t) => t.state !== 'exited').length;
    if (freeSpaces < 0 || freeSpaces > capacity || freeSpaces !== capacity - open) {
      throw new Error(`controller: free spaces ${freeSpaces} breaks the invariants (capacity ${capacity}, open tickets ${open})`);
    }
  }

  // The request loop: serves the inbox one request at a time under the counter lock.
  function receive(message, from) {
    inbox.push({ message, from });
    if (locked) return;
    locked = true;
    try {
      while (inbox.length > 0) {
        const next = inbox.shift();
        handle(next.message, next.from);
        checkInvariants();
      }
    } finally {
      locked = false;
    }
  }

  // Expires paid tickets as time passes, even if nobody asks about them.
  function sweep() {
    for (const ticket of book.values()) refresh(ticket);
  }

  return {
    capacity,
    graceSeconds,
    announce,
    sweep,
    free: () => freeSpaces,
    ticketBook: () => [...book.values()].map((t) => ({ ...t })),
  };
}
