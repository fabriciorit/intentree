import test from 'node:test';
import assert from 'node:assert/strict';
import { advanceTo, tickFor, MESSAGE_STATES } from '../src/message-states.js';

test('the ticks follow the state machine', () => {
  assert.deepEqual(MESSAGE_STATES.map(tickFor), ['draft', 'clock', 'one-grey', 'two-grey', 'two-blue']);
});

test('advanceTo walks one state at a time', () => {
  assert.deepEqual(advanceTo('sent', 'read'), [
    { from: 'sent', to: 'delivered' },
    { from: 'delivered', to: 'read' },
  ]);
});

test('a read notice can never skip delivered', () => {
  const states = advanceTo('queued', 'read').map((step) => step.to);
  assert.ok(states.indexOf('delivered') >= 0 && states.indexOf('delivered') < states.indexOf('read'));
});

test('a state never moves backwards or repeats', () => {
  assert.deepEqual(advanceTo('delivered', 'sent'), []);
  assert.deepEqual(advanceTo('read', 'read'), []);
  assert.throws(() => advanceTo('read', 'archived'));
});
