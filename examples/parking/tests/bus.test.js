import test from 'node:test';
import assert from 'node:assert/strict';
import { createClock } from '../domain/clock.js';
import { createBus } from '../domain/bus.js';

test('messages arrive in order and replies are delivered in the same pass', () => {
  const clock = createClock(0);
  const bus = createBus({ clock });
  const log = [];
  bus.register('b', (m) => { log.push(`b:${m.n}`); if (m.n === 1) bus.send('b', 'a', { type: 'x', n: 9 }); });
  bus.register('a', (m) => log.push(`a:${m.n}`));
  bus.send('a', 'b', { type: 'x', n: 1 });
  bus.send('a', 'b', { type: 'x', n: 2 });
  bus.deliver();
  assert.deepEqual(log, ['b:1', 'b:2', 'a:9']);
});

test('latency holds messages until the clock reaches them', () => {
  const clock = createClock(0);
  const bus = createBus({ clock });
  const log = [];
  bus.register('b', (m) => log.push(m.n));
  bus.setLatency(3);
  bus.send('a', 'b', { type: 'x', n: 1 });
  bus.deliver();
  assert.deepEqual(log, []);
  clock.advance(3);
  bus.deliver();
  assert.deepEqual(log, [1]);
});

test('an offline destination loses the message', () => {
  const clock = createClock(0);
  const bus = createBus({ clock });
  const log = [];
  const events = [];
  bus.onEvent((e) => events.push(e.kind));
  bus.register('b', (m) => log.push(m.n));
  bus.setOffline('b', true);
  bus.send('a', 'b', { type: 'x', n: 1 });
  bus.deliver();
  assert.deepEqual(log, []);
  assert.ok(events.includes('dropped'));
});
