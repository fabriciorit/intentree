import test from 'node:test';
import assert from 'node:assert/strict';
import { createBarrier, BARRIER_TRANSITIONS, TIMING } from '../domain/barrier.js';

function run(barrier, seconds, dt = 0.1) {
  for (let t = 0; t < seconds; t += dt) barrier.tick(dt);
}

test('the barrier cycles closed -> opening -> open -> closing -> closed', () => {
  const seen = [];
  const barrier = createBarrier({ onChange: (from, to) => seen.push(`${from}>${to}`) });
  barrier.open();
  run(barrier, TIMING.open + TIMING.hold + TIMING.close + 1);
  assert.deepEqual(seen, ['closed>opening', 'opening>open', 'open>closing', 'closing>closed']);
  assert.equal(barrier.openness, 0);
});

test('it never closes on a car: the sensor blocks the arm', () => {
  const barrier = createBarrier();
  barrier.open();
  run(barrier, TIMING.open + TIMING.hold - 0.1);
  assert.equal(barrier.state, 'open');
  run(barrier, 0.5);
  assert.equal(barrier.state, 'closing');
  barrier.setSensor(true);
  assert.equal(barrier.state, 'blocked');
  run(barrier, 30);
  assert.notEqual(barrier.state, 'closed');
  assert.notEqual(barrier.state, 'closing');
  assert.equal(barrier.openness, 1, 'the arm went back up');
  barrier.setSensor(false);
  run(barrier, 0.2);
  assert.equal(barrier.state, 'closing');
  run(barrier, TIMING.close + 0.5);
  assert.equal(barrier.state, 'closed');
});

test('a car under the arm keeps an open barrier open', () => {
  const barrier = createBarrier();
  barrier.open();
  run(barrier, TIMING.open + 0.1);
  barrier.setSensor(true);
  run(barrier, TIMING.hold + 20);
  assert.equal(barrier.state, 'open');
});

test('a new authorization while closing or blocked opens it again', () => {
  const barrier = createBarrier();
  barrier.open();
  run(barrier, TIMING.open + TIMING.hold + 0.5);
  assert.equal(barrier.state, 'closing');
  barrier.open();
  assert.equal(barrier.state, 'opening');
});

test('every transition is declared; undeclared moves cannot happen', () => {
  const allowed = new Set(BARRIER_TRANSITIONS.map(([a, b]) => `${a}>${b}`));
  const seen = [];
  const barrier = createBarrier({ onChange: (from, to) => seen.push(`${from}>${to}`) });
  for (let i = 0; i < 3000; i++) {
    if (i % 400 === 0) barrier.open();
    barrier.setSensor(i % 400 > 78 && i % 400 < 130);
    barrier.tick(0.1);
  }
  assert.ok(seen.length > 12);
  assert.ok(seen.includes('closing>blocked'));
  for (const move of seen) assert.ok(allowed.has(move), move);
});
