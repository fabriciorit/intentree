// The Ticket state machine: issued -> paid -> exited, plus expired when a paid ticket waits
// longer than the grace period before leaving (then it must be paid again).

export const TICKET_STATES = ['issued', 'paid', 'expired', 'exited'];
export const TICKET_TRANSITIONS = [
  ['issued', 'paid'],
  ['paid', 'exited'],
  ['paid', 'expired'],
  ['expired', 'paid'],
  ['exited', 'exited'], // a second use is refused and the ticket simply stays exited
];

export function createTicket(id, issuedAt, gate) {
  return { id, gate, state: 'issued', issuedAt, paidAt: null, exitedAt: null, paidTotal: 0 };
}

// The only way a ticket changes state; an illegal move (e.g. issued -> exited) throws.
export function moveTicket(ticket, to) {
  const legal = TICKET_TRANSITIONS.some(([from, next]) => from === ticket.state && next === to);
  if (!legal) throw new Error(`ticket ${ticket.id}: illegal move ${ticket.state} -> ${to}`);
  ticket.state = to;
}

// Total owed since the ticket was issued: each started block of time costs one rate.
export function totalFee(ticket, now, blockSeconds, ratePerBlock) {
  const blocks = Math.max(1, Math.ceil((now - ticket.issuedAt) / blockSeconds));
  return blocks * ratePerBlock;
}
