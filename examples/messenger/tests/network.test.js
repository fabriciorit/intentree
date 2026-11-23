import test from 'node:test';
import assert from 'node:assert/strict';
import { Clock } from '../src/clock.js';
import { Network, seededRandom } from '../src/network.js';

function setup(options) {
  const clock = new Clock();
  const events = [];
  const network = new Network(clock, { ...options, observe: (e) => events.push(e) });
  const inbox = { a: [], b: [] };
  network.attach('a', (p) => inbox.a.push(p));
  network.attach('b', (p) => inbox.b.push(p));
  return { clock, network, inbox, events };
}

test('a packet arrives after the latency, not before', () => {
  const { clock, network, inbox } = setup({ latencyMs: 400 });
  network.send('a', 'b', 'hi', { n: 1 });
  clock.advance(399);
  assert.equal(inbox.b.length, 0);
  clock.advance(1);
  assert.equal(inbox.b.length, 1);
  assert.equal(inbox.b[0].arriveAt - inbox.b[0].sentAt, 400);
});

test('the body is copied: the receiver cannot touch the sender data', () => {
  const { clock, network, inbox } = setup({ latencyMs: 10 });
  const body = { list: [1] };
  network.send('a', 'b', 'x', body);
  body.list.push(2);
  clock.advance(10);
  assert.deepEqual(inbox.b[0].body, { list: [1] });
});

test('an offline phone loses packets going in and out, and is told about the link', () => {
  const { clock, network, inbox, events } = setup({ latencyMs: 100 });
  const links = [];
  network.watchLink('b', (up) => links.push(up));
  network.setOffline('b', true);
  network.send('a', 'b', 'to-b', {});
  network.send('b', 'a', 'from-b', {});
  clock.advance(500);
  assert.equal(inbox.b.length + inbox.a.length, 0);
  assert.deepEqual(links, [false]);
  assert.equal(events.filter((e) => e.kind === 'drop').length, 2);
  network.setOffline('b', false);
  network.send('a', 'b', 'again', {});
  clock.advance(500);
  assert.equal(inbox.b.length, 1);
});

test('a packet already on its way is lost if the phone goes offline before it lands', () => {
  const { clock, network, inbox } = setup({ latencyMs: 100 });
  network.send('a', 'b', 'x', {});
  clock.advance(50);
  network.setOffline('b', true);
  clock.advance(100);
  assert.equal(inbox.b.length, 0);
});

test('the cellular channel ignores the Wi-Fi switch', () => {
  const { clock, network, inbox } = setup({ latencyMs: 100 });
  network.setOffline('b', true);
  network.send('a', 'b', 'push', {}, 'cellular');
  clock.advance(1000);
  assert.equal(inbox.b.length, 1);
});

test('loss is optional and repeatable with the same seed', () => {
  const run = () => {
    const { clock, network, inbox } = setup({ latencyMs: 10, lossRate: 0.5, random: seededRandom(3) });
    for (let i = 0; i < 40; i += 1) network.send('a', 'b', 'n', { i });
    clock.advance(100);
    return inbox.b.map((p) => p.body.i);
  };
  const first = run();
  assert.ok(first.length > 5 && first.length < 35);
  assert.deepEqual(run(), first);
  const { clock, network, inbox } = setup({ latencyMs: 10 });
  for (let i = 0; i < 40; i += 1) network.send('a', 'b', 'n', { i });
  clock.advance(100);
  assert.equal(inbox.b.length, 40);
});
