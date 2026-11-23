// A phone: the client side of the chat. It keeps a local outbox and a connection state, and
// talks to the server only through the network. The same code runs as "phone A" and "phone B".

import { advanceTo } from './message-states.js';

// Delay before the n-th retry (n starts at 1): doubles every time, up to a ceiling.
export function backoffDelay(attempt, baseMs, maxMs) {
  return Math.min(baseMs * 2 ** (attempt - 1), maxMs);
}

// What the chat shows, top to bottom: messages in server sequence order; messages the server
// has not numbered yet (still waiting in the outbox) come last, in the order they were written.
export function visibleMessages(messages) {
  const numbered = messages.filter((m) => m.seq !== null).sort((a, b) => a.seq - b.seq);
  const waiting = messages.filter((m) => m.seq === null).sort((a, b) => a.order - b.order);
  return [...numbered, ...waiting];
}

// A phone never marks its own message as read: only the other person's messages can be read here.
export function assertIncoming(message, self) {
  if (message.from === self) throw new Error('a phone never marks its own message as read');
}

// A message is shown once. The server resends until it hears back, so repeats are expected:
// a message id already known is not added again (returns null).
export function acceptDelivery(messages, incoming, now) {
  if (messages.has(incoming.id)) return null;
  const message = { ...incoming, state: 'sent', order: 0, createdAt: now };
  messages.set(message.id, message);
  return message;
}

const CONNECTION_MOVES = {
  offline: ['syncing'],
  syncing: ['online', 'offline'],
  online: ['offline'],
};

export class Phone {
  #name;
  #tag;
  #peer;
  #server;
  #clock;
  #network;
  #observe;
  #backoffBaseMs;
  #backoffMaxMs;
  #pingMs;
  #connection = 'offline';
  #messages = new Map(); // message id -> message
  #written = 0;
  #chatOpen = false;
  #notifications = 0;
  #pendingReads = new Set();
  #retryTimers = new Map(); // message id -> timer id
  #helloTimer = null;
  #helloAttempts = 0;
  #readTimer = null;
  #pingTimer = null;

  constructor({ name, tag, peer, clock, network, observe = () => {}, server = 'server', backoffBaseMs = 2000, backoffMaxMs = 8000, pingMs = 2000 }) {
    this.#name = name;
    this.#tag = tag;
    this.#peer = peer;
    this.#server = server;
    this.#clock = clock;
    this.#network = network;
    this.#observe = observe;
    this.#backoffBaseMs = backoffBaseMs;
    this.#backoffMaxMs = backoffMaxMs;
    this.#pingMs = pingMs;
    network.attach(name, (packet) => this.handle(packet));
    network.watchLink(name, (up) => this.#onLink(up));
  }

  get name() {
    return this.#name;
  }

  get connection() {
    return this.#connection;
  }

  get chatOpen() {
    return this.#chatOpen;
  }

  get notifications() {
    return this.#notifications;
  }

  messages() {
    return visibleMessages([...this.#messages.values()]);
  }

  unread() {
    return [...this.#messages.values()].filter((m) => m.from === this.#peer && m.state === 'delivered').length;
  }

  start() {
    if (!this.#network.isOffline(this.#name)) this.#onLink(true);
  }

  // ---- writing and sending

  compose(text) {
    this.#written += 1;
    const message = {
      id: `${this.#tag}-${this.#written}`,
      from: this.#name,
      to: this.#peer,
      text,
      seq: null,
      state: 'composing',
      order: this.#written,
      createdAt: this.#clock.now(),
    };
    this.#messages.set(message.id, message);
    this.#observe({ entity: this.#name, kind: 'state', detail: { machine: 'message', id: message.id, from: null, to: 'composing' } });
    return message;
  }

  // Putting a draft in the outbox. The message waits there until the server acknowledges it.
  submit(id) {
    const message = this.#messages.get(id);
    this.#move(message, 'queued');
    if (this.#connection === 'online') this.#transmit(message, 1);
    return message;
  }

  send(text) {
    this.openChat();
    return this.submit(this.compose(text).id);
  }

  #transmit(message, attempt) {
    this.#send('send', { message: { id: message.id, from: message.from, to: message.to, text: message.text } }, { id: message.id });
    const delay = backoffDelay(attempt, this.#backoffBaseMs, this.#backoffMaxMs);
    this.#retryTimers.set(
      message.id,
      this.#clock.after(delay, () => {
        if (message.state !== 'queued' || this.#connection !== 'online') return;
        this.#observe({ entity: this.#name, kind: 'retry', detail: { id: message.id, attempt: attempt + 1, delayMs: delay } });
        this.#transmit(message, attempt + 1);
      }),
    );
  }

  #flushOutbox() {
    for (const message of this.#messages.values()) {
      if (message.state === 'queued') this.#transmit(message, 1);
    }
  }

  // ---- reading

  openChat() {
    this.#chatOpen = true;
    this.#notifications = 0;
    for (const message of this.#messages.values()) {
      if (message.from === this.#peer && message.state === 'delivered') this.#markRead(message);
    }
    this.#flushReads();
  }

  closeChat() {
    this.#chatOpen = false;
  }

  #markRead(message) {
    assertIncoming(message, this.#name);
    this.#move(message, 'read');
    this.#pendingReads.add(message.id);
  }

  // Sends the read receipts that were not acknowledged yet, and backs off while they stay unacknowledged.
  #flushReads(attempt = 1) {
    this.#clock.cancel(this.#readTimer);
    if (this.#pendingReads.size === 0 || this.#connection !== 'online') return;
    this.#send('receipt', { kind: 'read', ids: [...this.#pendingReads] });
    const delay = backoffDelay(attempt, this.#backoffBaseMs, this.#backoffMaxMs);
    this.#readTimer = this.#clock.after(delay, () => this.#flushReads(attempt + 1));
  }

  // ---- packets in

  handle(packet) {
    this.#observe({ entity: this.#name, kind: 'receive', detail: { from: packet.from, type: packet.type } });
    if (packet.type === 'welcome') this.#onWelcome();
    if (packet.type === 'ack') this.#onAck(packet.body);
    if (packet.type === 'deliver') this.#onDeliver(packet.body.message);
    if (packet.type === 'status') this.#onStatus(packet.body);
    if (packet.type === 'receipt ack') this.#onReceiptAck(packet.body);
    if (packet.type === 'push') this.#onPush();
  }

  #onWelcome() {
    if (this.#connection !== 'syncing') return;
    this.#clock.cancel(this.#helloTimer);
    this.#setConnection('online');
    this.#notifications = 0;
    this.#startPing();
    this.#flushOutbox();
    this.#flushReads();
  }

  #onAck({ id, seq }) {
    const message = this.#messages.get(id);
    if (!message || message.from !== this.#name) return;
    message.seq = seq;
    this.#clock.cancel(this.#retryTimers.get(id));
    this.#retryTimers.delete(id);
    this.#move(message, 'sent');
  }

  // A message is shown once: the server resends until it hears back, so repeats are expected.
  #onDeliver(incoming) {
    const message = acceptDelivery(this.#messages, incoming, this.#clock.now());
    if (message) {
      this.#move(message, 'delivered');
      if (this.#chatOpen) this.#markRead(message);
    }
    this.#send('receipt', { kind: 'delivered', ids: [incoming.id] });
    if (this.#chatOpen) this.#flushReads();
  }

  #onStatus({ id, state, seq }) {
    const message = this.#messages.get(id);
    if (message && message.from === this.#name) {
      message.seq = seq;
      this.#clock.cancel(this.#retryTimers.get(id));
      this.#move(message, state);
    }
    this.#send('status ack', { key: `status:${id}`, state });
  }

  #onReceiptAck({ kind, ids }) {
    if (kind !== 'read') return;
    for (const id of ids) this.#pendingReads.delete(id);
    if (this.#pendingReads.size === 0) this.#clock.cancel(this.#readTimer);
  }

  // A push means "something is waiting". Online phones hear about it over the wire anyway.
  #onPush() {
    if (this.#connection === 'online') return;
    this.#notifications += 1;
    this.#observe({ entity: this.#name, kind: 'notify', detail: { count: this.#notifications } });
  }

  // ---- connection

  #onLink(up) {
    if (up && this.#connection === 'offline') {
      this.#setConnection('syncing');
      this.#helloAttempts = 0;
      this.#sayHello();
    }
    if (!up && this.#connection !== 'offline') {
      this.#setConnection('offline');
      this.#clock.cancel(this.#helloTimer);
      this.#clock.cancel(this.#pingTimer);
      this.#clock.cancel(this.#readTimer);
      for (const timer of this.#retryTimers.values()) this.#clock.cancel(timer);
      this.#retryTimers.clear();
    }
  }

  #sayHello() {
    this.#helloAttempts += 1;
    this.#send('hello', {});
    const delay = backoffDelay(this.#helloAttempts, this.#backoffBaseMs, this.#backoffMaxMs);
    this.#helloTimer = this.#clock.after(delay, () => {
      if (this.#connection === 'syncing') this.#sayHello();
    });
  }

  #startPing() {
    this.#pingTimer = this.#clock.after(this.#pingMs, () => {
      if (this.#connection !== 'online') return;
      this.#send('ping', {});
      this.#startPing();
    });
  }

  #setConnection(next) {
    if (!CONNECTION_MOVES[this.#connection].includes(next)) throw new Error(`connection cannot go ${this.#connection} -> ${next}`);
    const from = this.#connection;
    this.#connection = next;
    this.#observe({ entity: this.#name, kind: 'state', detail: { machine: 'connection', from, to: next } });
  }

  // ---- helpers

  #move(message, target) {
    for (const step of advanceTo(message.state, target)) {
      message.state = step.to;
      this.#observe({ entity: this.#name, kind: 'state', detail: { machine: 'message', id: message.id, from: step.from, to: step.to } });
    }
  }

  #send(type, body, extra = {}) {
    this.#observe({ entity: this.#name, kind: 'send', detail: { to: this.#server, type, ...extra } });
    this.#network.send(this.#name, this.#server, type, body);
  }
}
