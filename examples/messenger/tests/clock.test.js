import test from 'node:test';
import assert from 'node:assert/strict';
import { Clock } from '../src/clock.js';

test('timers fire in due order, ties in scheduling order', () => {
  const clock = new Clock();
  const fired = [];
  clock.after(50, () => fired.push('c'));
  clock.after(10, () => fired.push('a'));
  clock.after(10, () => fired.push('b'));
  clock.advance(100);
  assert.deepEqual(fired, ['a', 'b', 'c']);
  assert.equal(clock.now(), 100);
});

test('a timer sees the time it was due, not the end of the step', () => {
  const clock = new Clock();
  let seen = null;
  clock.after(30, () => (seen = clock.now()));
  clock.advance(100);
  assert.equal(seen, 30);
});

test('a cancelled timer never fires and timers can schedule timers', () => {
  const clock = new Clock();
  const fired = [];
  const id = clock.after(10, () => fired.push('no'));
  clock.cancel(id);
  clock.after(5, () => clock.after(5, () => fired.push('chain')));
  clock.advance(20);
  assert.deepEqual(fired, ['chain']);
  assert.equal(clock.pending(), 0);
});
