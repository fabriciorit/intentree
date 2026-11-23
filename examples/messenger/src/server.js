// The server. It orders messages per conversation, acknowledges them, and keeps each one
// until the recipient confirms receipt. It also tracks which phones are online, because that
// decides whether a waiting message gets a push notification.

import { advanceTo } from './message-states.js';

export function conversationId(a, b) {
  return [a, b].sort().join(' + ');
}

// The sequence number of the next message in a conversation: it only ever goes up.
export function assignSequence(conversation) {
  conversation.lastSeq += 1;
  return conversation.lastSeq;
}

// The only way a stored message is let go: its recipient confirmed receipt. The copy kept for
// resending and the entry waiting in the recipient's mailbox go together.
export function confirmReceipt(conversation, mailbox, id) {
  conversation.pending.delete(id);
  mailbox.delete(`deliver:${id}`);
}

// The messages worth a push: new ones, not yet nudged, and only if the phone is offline.
// For a phone that is online there are none: the push gateway is not used for it.
export function pushCandidates(presence, mailbox) {
  if (presence === 'online') return [];
  return [...mailbox.values()].filter((item) => item.type === 'deliver' && !item.pushed);
}

export class Server {
  #clock;
  #network;
  #observe;
  #address;
  #gateway;
  #retryMs;
  #presenceTimeoutMs;
  #conversations = new Map(); // conversation id -> { lastSeq, pending: Map(message id -> stored message) }
  #index = new Map(); // message id -> { conversation, from, to, seq, state }, kept for receipts
  #presence = new Map(); // phone -> 'online' | 'offline'
  #watchdogs = new Map(); // phone -> timer id
  #mailboxes = new Map(); // phone -> Map(key -> { type, body, pushed })
  #retries = new Map(); // phone -> timer id

  constructor({ clock, network, observe = () => {}, address = 'server', gateway = 'push gateway', retryMs = 2000, presenceTimeoutMs = 5000 }) {
    this.#clock = clock;
    this.#network = network;
    this.#observe = observe;
    this.#address = address;
    this.#gateway = gateway;
    this.#retryMs = retryMs;
    this.#presenceTimeoutMs = presenceTimeoutMs;
    network.attach(address, (packet) => this.handle(packet));
  }

  presenceOf(phone) {
    return this.#presence.get(phone) ?? 'offline';
  }

  snapshot() {
    const conversations = [...this.#conversations].map(([id, c]) => ({ id, lastSeq: c.lastSeq, stored: c.pending.size }));
    const waiting = {};
    for (const [phone, box] of this.#mailboxes) waiting[phone] = box.size;
    return { conversations, presence: Object.fromEntries(this.#presence), waiting };
  }

  handle(packet) {
    this.#observe({ entity: this.#address, kind: 'receive', detail: { from: packet.from, type: packet.type } });
    if (packet.type === 'hello') this.#send(packet.from, 'welcome', {});
    if (packet.type === 'send') this.#onSend(packet);
    if (packet.type === 'receipt') this.#onReceipt(packet);
    if (packet.type === 'status ack') this.#onStatusAck(packet);
    // anything a phone says proves it is online right now
    const changed = this.#heard(packet.from);
    // a phone that says hello wants what is waiting for it, even if we thought it was online
    if (packet.type === 'hello' && !changed) this.#flush(packet.from);
  }

  #send(to, type, body, extra = {}) {
    this.#observe({ entity: this.#address, kind: 'send', detail: { to, type, ...extra } });
    return this.#network.send(this.#address, to, type, body, to === this.#gateway ? 'backbone' : 'wifi');
  }

  #move(entry, id, target) {
    for (const step of advanceTo(entry.state, target)) {
      entry.state = step.to;
      this.#observe({ entity: this.#address, kind: 'state', detail: { machine: 'message', id, from: step.from, to: step.to } });
      if (step.to === 'delivered') this.#release(entry, id);
    }
  }

  // The same message id can arrive twice (the phone retried): store it once, acknowledge again.
  #onSend(packet) {
    const message = packet.body.message;
    if (message.from !== packet.from) return;
    const known = this.#index.get(message.id);
    if (known) {
      this.#send(packet.from, 'ack', { id: message.id, seq: known.seq }, { id: message.id });
      return;
    }
    const id = conversationId(message.from, message.to);
    if (!this.#conversations.has(id)) this.#conversations.set(id, { lastSeq: 0, pending: new Map() });
    const conversation = this.#conversations.get(id);
    const stored = { ...message, seq: assignSequence(conversation) };
    conversation.pending.set(stored.id, stored);
    const entry = { conversation: id, from: stored.from, to: stored.to, seq: stored.seq, state: 'queued' };
    this.#index.set(stored.id, entry);
    this.#observe({ entity: this.#address, kind: 'store', detail: { id: stored.id, seq: stored.seq } });
    this.#move(entry, stored.id, 'sent');
    this.#send(packet.from, 'ack', { id: stored.id, seq: stored.seq }, { id: stored.id });
    this.#enqueue(stored.to, `deliver:${stored.id}`, 'deliver', { message: stored });
  }

  // A receipt from the recipient: delivered or read. Read before delivered walks through delivered.
  #onReceipt(packet) {
    const { kind, ids } = packet.body;
    if (kind !== 'delivered' && kind !== 'read') return;
    for (const id of ids) {
      const entry = this.#index.get(id);
      if (!entry || entry.to !== packet.from) continue;
      const before = entry.state;
      this.#move(entry, id, kind);
      if (entry.state !== before) this.#enqueue(entry.from, `status:${id}`, 'status', { id, state: entry.state, seq: entry.seq });
    }
    this.#send(packet.from, 'receipt ack', { kind, ids });
  }

  // The ack names the state it saw: a newer status for the same message must stay in the mailbox.
  #onStatusAck(packet) {
    const box = this.#mailbox(packet.from);
    const item = box.get(packet.body.key);
    if (item && item.body.state === packet.body.state) box.delete(packet.body.key);
  }

  #release(entry, id) {
    confirmReceipt(this.#conversations.get(entry.conversation), this.#mailbox(entry.to), id);
    this.#observe({ entity: this.#address, kind: 'discard', detail: { id } });
  }

  #mailbox(phone) {
    if (!this.#mailboxes.has(phone)) this.#mailboxes.set(phone, new Map());
    return this.#mailboxes.get(phone);
  }

  #enqueue(phone, key, type, body) {
    this.#mailbox(phone).set(key, { type, body, pushed: false });
    this.#flush(phone);
  }

  // Sends everything the phone has not confirmed yet, and keeps retrying while it stays online.
  #flush(phone) {
    const box = this.#mailbox(phone);
    this.#clock.cancel(this.#retries.get(phone));
    this.#retries.delete(phone);
    if (box.size === 0) return;
    if (this.presenceOf(phone) === 'offline') {
      this.#notifyOffline(phone, box);
      return;
    }
    for (const [key, item] of box) this.#send(phone, item.type, item.body, { id: item.body.id ?? item.body.message?.id, key });
    this.#retries.set(phone, this.#clock.after(this.#retryMs, () => this.#flush(phone)));
  }

  // The push gateway is for phones that are offline, and only for new messages, once each.
  #notifyOffline(phone, box) {
    const fresh = pushCandidates(this.presenceOf(phone), box);
    if (fresh.length === 0) return;
    for (const item of fresh) item.pushed = true;
    this.#send(this.#gateway, 'push request', { to: phone, preview: 'New message' });
  }

  #heard(phone) {
    if (phone === this.#gateway) return false;
    this.#clock.cancel(this.#watchdogs.get(phone));
    this.#watchdogs.set(phone, this.#clock.after(this.#presenceTimeoutMs, () => this.#setPresence(phone, 'offline')));
    return this.#setPresence(phone, 'online');
  }

  #setPresence(phone, next) {
    const before = this.presenceOf(phone);
    if (before === next) return false;
    this.#presence.set(phone, next);
    this.#observe({ entity: this.#address, kind: 'state', detail: { machine: 'presence', phone, from: before, to: next } });
    this.#flush(phone);
    return true;
  }
}
