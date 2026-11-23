// The wire protocol: who the actors are and which messages they exchange.
// Every actor talks to the others only with these names, through the message bus.

export const ENDPOINT = Object.freeze({
  controller: 'controller',
  entry1: 'entry gate 1',
  entry2: 'entry gate 2',
  exit: 'exit gate',
  kiosk: 'payment kiosk',
});

export const MSG = Object.freeze({
  REQUEST_SPACE: 'request space',
  TICKET_GRANTED: 'ticket granted',
  GARAGE_FULL: 'garage full',
  FREE_COUNT: 'free count',
  PAY: 'pay',
  PAYMENT_ACCEPTED: 'payment accepted',
  PAYMENT_REJECTED: 'payment rejected',
  EXIT_REQUEST: 'exit request',
  EXIT_GRANTED: 'exit granted',
  EXIT_DENIED: 'exit denied',
});

export const DEFAULTS = Object.freeze({
  capacity: 12,
  graceSeconds: 15 * 60, // a paid ticket must leave within 15 simulated minutes
  ratePerBlock: 1.5, // price of each started block of 15 minutes
  blockSeconds: 15 * 60,
  requestTimeout: 10, // seconds before a gate gives up waiting for the controller
});
