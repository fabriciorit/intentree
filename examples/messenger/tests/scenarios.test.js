import test from 'node:test';
import assert from 'node:assert/strict';
import { createWorld } from '../src/world.js';
import { SCENARIOS, runScenario } from '../src/scenarios.js';

for (const scenario of SCENARIOS) {
  test(`scenario "${scenario.label}" ends with every message read on both phones and nothing left on the server`, () => {
    const w = createWorld();
    runScenario(w, scenario);
    w.advance(90000);
    for (const phone of [w.phones.A, w.phones.B]) {
      assert.ok(phone.messages().length > 0);
      assert.ok(phone.messages().every((m) => m.state === 'read'), `${phone.name}: ${phone.messages().map((m) => m.state)}`);
    }
    assert.deepEqual(w.server.snapshot().conversations.map((c) => c.stored), [0]);
    assert.deepEqual(w.server.snapshot().waiting, { 'phone A': 0, 'phone B': 0 });
  });
}

test('the offline scenario uses the push gateway exactly once', () => {
  const w = createWorld();
  runScenario(w, SCENARIOS[0]);
  w.advance(30000);
  assert.equal(w.gateway.pushesTo('phone B'), 1);
});
