import test from 'node:test';
import assert from 'node:assert/strict';
import { createGarage } from '../domain/garage.js';
import { DEFAULTS } from '../domain/protocol.js';
import { seeded } from './helpers.js';

function garage(options = {}) {
  return createGarage({ random: seeded(3), arrivalsPerMinute: 0, ...options });
}

const states = (g) => g.controller.ticketBook().map((t) => t.state);

test('a car parks, pays and leaves; its space is freed', () => {
  const g = garage();
  g.actions.arriveAtEntry1();
  g.advance(40);
  assert.equal(g.controller.free(), DEFAULTS.capacity - 1);
  assert.deepEqual(states(g), ['issued']);
  g.actions.payTicket();
  g.advance(40);
  assert.deepEqual(states(g), ['paid']);
  g.actions.leave();
  g.advance(40);
  assert.deepEqual(states(g), ['exited']);
  assert.equal(g.controller.free(), DEFAULTS.capacity);
  assert.equal(g.traffic.cars.length, 0);
});

test('trying to leave without paying is refused and the car stays', () => {
  const g = garage();
  g.actions.arriveAtEntry2();
  g.actions.tryLeaveUnpaid();
  g.advance(80);
  assert.deepEqual(states(g), ['issued']);
  assert.equal(g.exit.barrier.state, 'closed');
  assert.equal(g.traffic.cars.length, 1);
  assert.equal(g.traffic.cars[0].phase, 'parked');
});

test('after the grace period the ticket expires and the car must pay again', () => {
  const g = garage();
  g.actions.arriveAtEntry1();
  g.actions.payTicket();
  g.advance(60);
  assert.deepEqual(states(g), ['paid']);
  g.actions.waitPastGrace();
  assert.deepEqual(states(g), ['expired']);
  g.actions.leave();
  g.advance(60);
  assert.deepEqual(states(g), ['expired'], 'refused at the exit');
  g.actions.payTicket();
  g.advance(40);
  assert.deepEqual(states(g), ['paid']);
  g.actions.leave();
  g.advance(60);
  assert.deepEqual(states(g), ['exited']);
});

test('rush hour at the last spaces: no overselling, extra cars see FULL', () => {
  const g = garage();
  g.actions.rushHour();
  const checks = [];
  for (let i = 0; i < 120; i++) {
    g.advance(0.5);
    checks.push(g.controller.free());
  }
  assert.ok(checks.every((n) => n >= 0 && n <= DEFAULTS.capacity));
  assert.equal(g.controller.free(), 0);
  assert.equal(g.controller.ticketBook().length, DEFAULTS.capacity);
  assert.equal(g.entry1.text, 'FULL');
  assert.equal(g.entry2.text, 'FULL');
  const parked = g.traffic.cars.filter((c) => c.phase === 'parked');
  assert.equal(new Set(parked.map((c) => c.slot.id)).size, parked.length, 'no two cars share a space');
});

test('the barrier never closes on a pedestrian', () => {
  const g = garage();
  g.actions.arriveAtEntry1();
  g.advance(3);
  const seen = [];
  g.onEvent((e) => { if (e.kind === 'state' && e.machine === 'Barrier') seen.push(`${e.entity}:${e.from}>${e.to}`); });
  g.actions.obstacleUnderBarrier();
  g.advance(30);
  assert.ok(seen.includes('entry gate 1:closing>blocked'));
  const blocked = seen.indexOf('entry gate 1:closing>blocked');
  assert.equal(seen[blocked + 1], 'entry gate 1:blocked>closing', 'it only resumes once the obstacle is gone');
  assert.equal(g.entry1.barrier.state, 'closed');
});

test('the obstacle action makes a car come in when every barrier is down', () => {
  const g = garage();
  g.actions.obstacleUnderBarrier();
  const seen = [];
  g.onEvent((e) => { if (e.kind === 'state' && e.to === 'blocked') seen.push(e.entity); });
  g.advance(60);
  assert.deepEqual(seen, ['entry gate 1']);
});

test('a long random day keeps every invariant', () => {
  const g = garage({ arrivalsPerMinute: 9 });
  let blocked = 0;
  g.onEvent((e) => { if (e.kind === 'state' && e.to === 'blocked') blocked += 1; });
  for (let i = 0; i < 4000; i++) {
    g.advance(1);
    const free = g.controller.free();
    assert.ok(free >= 0 && free <= DEFAULTS.capacity);
    const open = g.controller.ticketBook().filter((t) => t.state !== 'exited').length;
    assert.equal(free, DEFAULTS.capacity - open);
    if (i % 1000 === 500) g.actions.rushHour();
  }
  const exited = g.controller.ticketBook().filter((t) => t.state === 'exited').length;
  assert.ok(exited > 20, `only ${exited} cars left`);
});

test('latency and an offline controller never oversell or open a barrier without a ticket', () => {
  const g = garage();
  g.setLatency(2);
  g.actions.rushHour();
  g.advance(60);
  g.setControllerOffline(true);
  g.actions.arriveAtEntry1();
  g.advance(60);
  g.setControllerOffline(false);
  g.advance(200);
  const book = g.controller.ticketBook();
  assert.ok(book.length <= DEFAULTS.capacity);
  assert.ok(g.controller.free() >= 0);
});
