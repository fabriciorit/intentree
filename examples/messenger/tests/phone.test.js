import test from 'node:test';
import assert from 'node:assert/strict';
import { createWorld } from '../src/world.js';
import { backoffDelay, visibleMessages, acceptDelivery, assertIncoming } from '../src/phone.js';

const states = (phone) => phone.messages().map((m) => `${m.id}:${m.state}`);

test('a message climbs the ticks: queued, sent, delivered, read', () => {
  const w = createWorld({ latencyMs: 100 });
  const m = w.control.sendMessage('A', 'hello');
  assert.equal(m.state, 'queued');
  w.advance(250);
  assert.equal(m.state, 'sent');
  w.advance(500);
  assert.equal(m.state, 'delivered');
  w.control.openChat('B');
  w.advance(500);
  assert.equal(m.state, 'read');
  assert.equal(w.phones.B.messages()[0].state, 'read');
});

test('while B has the chat open, a message is read as soon as it lands', () => {
  const w = createWorld({ latencyMs: 100 });
  w.control.openChat('B');
  w.control.sendMessage('A', 'hello');
  w.advance(1500);
  assert.deepEqual(states(w.phones.A), ['A-1:read']);
});

test('a phone never marks its own message as read', () => {
  const w = createWorld({ latencyMs: 100 });
  w.control.sendMessage('A', 'mine');
  w.advance(1000);
  w.control.openChat('A');
  w.advance(1000);
  assert.notEqual(w.phones.A.messages()[0].state, 'read');
  assert.equal(w.phones.A.unread(), 0);
});

test('written while offline: waits in the outbox with a clock, then goes out when back online', () => {
  const w = createWorld({ latencyMs: 100 });
  w.control.goOffline('A');
  const m = w.control.sendMessage('A', 'later');
  w.advance(10000);
  assert.equal(m.state, 'queued');
  assert.equal(w.server.snapshot().conversations.length, 0);
  w.control.goOnline('A');
  w.advance(1000);
  assert.equal(m.state, 'delivered'); // B was online all along
  assert.equal(w.phones.B.messages().length, 1);
});

test('the outbox retries with growing delays when acknowledgements do not come', () => {
  assert.deepEqual([1, 2, 3, 4, 5].map((n) => backoffDelay(n, 2000, 8000)), [2000, 4000, 8000, 8000, 8000]);
  const w = createWorld({ latencyMs: 100 });
  w.control.setLoss(1);
  w.control.sendMessage('A', 'lost');
  w.advance(20000);
  const retries = w.log.filter((e) => e.entity === 'phone A' && e.kind === 'retry');
  assert.deepEqual(retries.map((e) => e.detail.delayMs), [2000, 4000, 8000]);
  w.control.setLoss(0);
  w.advance(20000);
  assert.equal(w.phones.A.messages()[0].state, 'delivered');
});

test('a message is never delivered twice, even when everything is resent', () => {
  const w = createWorld({ latencyMs: 700 }); // round trip longer than the first retry delay
  w.control.sendMessage('A', 'once');
  w.advance(30000);
  assert.equal(w.phones.B.messages().length, 1);
  assert.equal(w.server.snapshot().conversations[0].lastSeq, 1);
  const delivered = w.log.filter((e) => e.entity === 'phone B' && e.kind === 'state' && e.detail.to === 'delivered');
  assert.equal(delivered.length, 1);
});

test('both phones see the same order, the one the server decided', () => {
  const w = createWorld({ latencyMs: 400 });
  w.control.openChat('A');
  w.control.openChat('B');
  w.control.sendMessage('A', 'a1');
  w.control.sendMessage('B', 'b1');
  w.control.sendMessage('A', 'a2');
  w.advance(5000);
  const order = (phone) => phone.messages().map((m) => m.text);
  assert.deepEqual(order(w.phones.A), order(w.phones.B));
  assert.deepEqual(order(w.phones.A).length, 3);
  const seqs = w.phones.A.messages().map((m) => m.seq);
  assert.deepEqual(seqs, [1, 2, 3]);
});

test('messages without a number yet are shown last, in writing order', () => {
  const shown = visibleMessages([
    { id: 'x3', seq: null, order: 3 },
    { id: 'n2', seq: 2, order: 9 },
    { id: 'x2', seq: null, order: 2 },
    { id: 'n1', seq: 1, order: 8 },
  ]);
  assert.deepEqual(shown.map((m) => m.id), ['n1', 'n2', 'x2', 'x3']);
});

test('push wakes an offline phone, and the message arrives when it reconnects', () => {
  const w = createWorld({ latencyMs: 100 });
  w.control.goOffline('B');
  w.control.sendMessage('A', 'are you there?');
  w.advance(9000);
  assert.equal(w.phones.B.notifications, 1);
  assert.equal(w.phones.B.messages().length, 0);
  assert.equal(w.phones.A.messages()[0].state, 'sent');
  w.control.goOnline('B');
  w.advance(2000);
  assert.equal(w.phones.B.messages().length, 1);
  assert.equal(w.phones.B.notifications, 0);
  assert.equal(w.phones.A.messages()[0].state, 'delivered');
});

test('the connection goes offline, syncing, online', () => {
  const w = createWorld({ latencyMs: 100 });
  assert.equal(w.phones.B.connection, 'online');
  w.control.goOffline('B');
  assert.equal(w.phones.B.connection, 'offline');
  w.control.goOnline('B');
  assert.equal(w.phones.B.connection, 'syncing');
  w.advance(500);
  assert.equal(w.phones.B.connection, 'online');
});

test('a message is never read before it is delivered, on any run of the flaky network', () => {
  for (const seed of [1, 2, 3, 4, 5, 6]) {
    const w = createWorld({ seed });
    w.control.flakyNetwork();
    w.control.openChat('B');
    for (let i = 0; i < 4; i += 1) w.control.sendMessage(i % 2 ? 'B' : 'A');
    w.advance(120000);
    const seen = new Map();
    for (const e of w.log.filter((x) => x.kind === 'state' && x.detail.machine === 'message')) {
      const list = seen.get(`${e.entity}/${e.detail.id}`) ?? [];
      list.push(e.detail.to);
      seen.set(`${e.entity}/${e.detail.id}`, list);
    }
    for (const [key, list] of seen) {
      const read = list.indexOf('read');
      if (read >= 0) assert.ok(list.indexOf('delivered') >= 0 && list.indexOf('delivered') < read, `${key}: ${list}`);
    }
    for (const phone of [w.phones.A, w.phones.B]) assert.deepEqual(phone.messages().map((m) => m.state), phone.messages().map(() => 'read'), `seed ${seed}`);
  }
});

test('acceptDelivery adds a message once and refuses a repeat', () => {
  const messages = new Map();
  const first = acceptDelivery(messages, { id: 'A-1', from: 'phone A', to: 'phone B', text: 'x', seq: 1 }, 5);
  assert.equal(first.state, 'sent');
  assert.equal(acceptDelivery(messages, { id: 'A-1', from: 'phone A', to: 'phone B', text: 'x', seq: 1 }, 9), null);
  assert.equal(messages.size, 1);
});

test('assertIncoming refuses a phone reading its own message', () => {
  assert.throws(() => assertIncoming({ from: 'phone A' }, 'phone A'));
  assert.doesNotThrow(() => assertIncoming({ from: 'phone B' }, 'phone A'));
});
