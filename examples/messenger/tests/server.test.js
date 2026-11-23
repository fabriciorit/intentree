import test from 'node:test';
import assert from 'node:assert/strict';
import { Clock } from '../src/clock.js';
import { Network } from '../src/network.js';
import { Server, assignSequence, conversationId, pushCandidates, confirmReceipt } from '../src/server.js';
import { PushGateway } from '../src/push-gateway.js';

// Bare-bones phones that only record what the server says to them.
function setup() {
  const clock = new Clock();
  const network = new Network(clock, { latencyMs: 50 });
  const server = new Server({ clock, network });
  const gateway = new PushGateway({ network });
  const inbox = { 'phone A': [], 'phone B': [] };
  for (const name of Object.keys(inbox)) network.attach(name, (p) => inbox[name].push(p));
  const say = (from, type, body) => network.send(from, 'server', type, body);
  const message = (id, from, to, text = 'hi') => ({ id, from, to, text });
  const ofType = (name, type) => inbox[name].filter((p) => p.type === type);
  return { clock, network, server, gateway, inbox, say, message, ofType };
}

test('sequence numbers go up by one per conversation and start again in another one', () => {
  const c = { lastSeq: 0 };
  assert.deepEqual([assignSequence(c), assignSequence(c), assignSequence(c)], [1, 2, 3]);
  assert.equal(conversationId('phone B', 'phone A'), conversationId('phone A', 'phone B'));
});

test('the server numbers messages in arrival order and acknowledges each', () => {
  const t = setup();
  t.say('phone A', 'hello', {});
  t.say('phone B', 'hello', {});
  t.clock.advance(200);
  t.say('phone B', 'send', { message: t.message('B-1', 'phone B', 'phone A') });
  t.clock.advance(10);
  t.say('phone A', 'send', { message: t.message('A-1', 'phone A', 'phone B') });
  t.clock.advance(300);
  assert.deepEqual(t.ofType('phone B', 'ack').map((p) => [p.body.id, p.body.seq]), [['B-1', 1]]);
  assert.deepEqual(t.ofType('phone A', 'ack').map((p) => [p.body.id, p.body.seq]), [['A-1', 2]]);
});

test('the same message id is stored once and acknowledged again with the same number', () => {
  const t = setup();
  t.say('phone A', 'hello', {});
  t.clock.advance(100);
  const send = () => t.say('phone A', 'send', { message: t.message('A-1', 'phone A', 'phone B') });
  send();
  t.clock.advance(100);
  send();
  t.clock.advance(100);
  const acks = t.ofType('phone A', 'ack');
  assert.equal(acks.length, 2);
  assert.deepEqual(acks.map((p) => p.body.seq), [1, 1]);
  assert.equal(t.server.snapshot().conversations[0].lastSeq, 1);
});

test('the server keeps a message and resends it until the recipient confirms receipt', () => {
  const t = setup();
  t.say('phone A', 'hello', {});
  t.say('phone B', 'hello', {});
  t.clock.advance(200);
  t.say('phone A', 'send', { message: t.message('A-1', 'phone A', 'phone B') });
  t.clock.advance(5000);
  // phone B is only a recorder: it never confirms, so the server keeps resending
  assert.ok(t.ofType('phone B', 'deliver').length >= 2);
  assert.equal(t.server.snapshot().conversations[0].stored, 1);
  t.say('phone B', 'receipt', { kind: 'delivered', ids: ['A-1'] });
  t.clock.advance(100);
  assert.equal(t.server.snapshot().conversations[0].stored, 0);
  const resent = t.ofType('phone B', 'deliver').length;
  t.clock.advance(5000);
  assert.equal(t.ofType('phone B', 'deliver').length, resent);
});

test('only the addressed recipient can confirm a message', () => {
  const t = setup();
  t.say('phone A', 'hello', {});
  t.clock.advance(100);
  t.say('phone A', 'send', { message: t.message('A-1', 'phone A', 'phone B') });
  t.clock.advance(100);
  t.say('phone A', 'receipt', { kind: 'delivered', ids: ['A-1'] });
  t.clock.advance(100);
  assert.equal(t.server.snapshot().conversations[0].stored, 1);
});

test('a read notice that outruns the delivered notice still passes through delivered', () => {
  const t = setup();
  t.say('phone A', 'hello', {});
  t.say('phone B', 'hello', {});
  t.clock.advance(200);
  t.say('phone A', 'send', { message: t.message('A-1', 'phone A', 'phone B') });
  t.clock.advance(200);
  t.say('phone B', 'receipt', { kind: 'read', ids: ['A-1'] });
  t.clock.advance(300);
  assert.equal(t.server.snapshot().conversations[0].stored, 0);
  const states = t.ofType('phone A', 'status').map((p) => p.body.state);
  assert.equal(states.at(-1), 'read');
});

test('the push gateway is used only when the recipient is offline', () => {
  const t = setup();
  t.say('phone A', 'hello', {});
  t.say('phone B', 'hello', {});
  t.clock.advance(200);
  t.say('phone A', 'send', { message: t.message('A-1', 'phone A', 'phone B') });
  t.clock.advance(1000);
  assert.equal(t.gateway.total(), 0, 'B is online: no push');
  t.clock.advance(10000); // B stops pinging, the server notices
  assert.equal(t.server.presenceOf('phone B'), 'offline');
  assert.equal(t.gateway.pushesTo('phone B'), 1);
  assert.equal(t.ofType('phone B', 'push').length, 1);
  t.clock.advance(20000);
  assert.equal(t.gateway.pushesTo('phone B'), 1, 'one nudge per message');
});

test('the gateway obeys only the server', () => {
  const t = setup();
  t.say('phone A', 'push request', { to: 'phone B', preview: 'spam' });
  t.clock.advance(1000);
  assert.equal(t.gateway.total(), 0);
});

test('pushCandidates: nothing for an online phone, only new unnotified deliveries for an offline one', () => {
  const box = new Map([
    ['deliver:A-1', { type: 'deliver', body: {}, pushed: false }],
    ['deliver:A-2', { type: 'deliver', body: {}, pushed: true }],
    ['status:B-1', { type: 'status', body: {}, pushed: false }],
  ]);
  assert.deepEqual(pushCandidates('online', box), []);
  assert.deepEqual(pushCandidates('offline', box).length, 1);
});

test('confirmReceipt lets go of the stored copy and the waiting delivery together', () => {
  const conversation = { lastSeq: 1, pending: new Map([['A-1', { id: 'A-1' }]]) };
  const mailbox = new Map([['deliver:A-1', {}], ['deliver:A-2', {}]]);
  confirmReceipt(conversation, mailbox, 'A-1');
  assert.equal(conversation.pending.size, 0);
  assert.deepEqual([...mailbox.keys()], ['deliver:A-2']);
});
