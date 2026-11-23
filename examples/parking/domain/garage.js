// The garage, assembled: builds every actor, wires them to one message bus and steps the
// simulation. It holds no rules of its own; it is also the set of actions a person (or the
// tour) can trigger.

import { DEFAULTS, ENDPOINT } from './protocol.js';
import { createClock } from './clock.js';
import { createBus } from './bus.js';
import { createController } from './controller.js';
import { createEntryGate1 } from './entry-gate-1.js';
import { createEntryGate2 } from './entry-gate-2.js';
import { createExitGate } from './exit-gate.js';
import { createKiosk } from './kiosk.js';
import { createTraffic } from './traffic.js';

export function createGarage({ capacity = DEFAULTS.capacity, random = Math.random, arrivalsPerMinute = 4 } = {}) {
  const clock = createClock();
  const bus = createBus({ clock });
  const controller = createController({ bus, clock, capacity });
  const entry1 = createEntryGate1({ bus });
  const entry2 = createEntryGate2({ bus });
  const exit = createExitGate({ bus });
  const kiosk = createKiosk({ bus });
  const traffic = createTraffic({ entries: [entry1, entry2], exit, kiosk, bus, random });
  traffic.setArrivalsPerMinute(arrivalsPerMinute);
  controller.announce();
  bus.deliver();

  function step(dt) {
    clock.advance(dt);
    bus.deliver();
    for (const actor of [entry1, entry2, exit, kiosk]) actor.tick(dt);
    controller.sweep();
    traffic.step(dt);
    bus.deliver();
  }

  function advance(seconds, maxStep = 0.25) {
    for (let left = seconds; left > 1e-9; left -= maxStep) step(Math.min(maxStep, left));
  }

  const arrive = (lane, mode = 'manual', dwell) => traffic.spawn(lane, { mode, dwell });

  const actions = {
    arriveAtEntry1: () => arrive(0),
    arriveAtEntry2: () => arrive(1),
    // A burst at the last free spaces: more cars than spaces, alternating between the entries.
    rushHour() {
      const spaces = Math.max(0, controller.free() - traffic.queued());
      const cars = spaces + 2;
      const first = random() < 0.5 ? 0 : 1;
      for (let i = 0; i < cars; i++) arrive((first + i) % 2, 'auto', 150 + random() * 150);
      return cars;
    },
    payTicket: () => traffic.request('pay'),
    tryLeaveUnpaid: () => traffic.request('tryLeaveUnpaid'),
    leave: () => traffic.request('leave'),
    // 16 simulated minutes at once: every paid ticket still waiting passes the grace period.
    waitPastGrace() {
      traffic.pauseArrivals(true);
      advance(DEFAULTS.graceSeconds + 60, 0.5);
      traffic.pauseArrivals(false);
    },
    // Someone steps under the arm as it starts to lower: the barrier must not close on them.
    // If every barrier is down, a car comes in so that one of them moves.
    obstacleUnderBarrier() {
      const moving = traffic.armObstacle();
      if (!moving) arrive(0, 'auto', 20);
    },
    toggleControllerOffline() {
      const value = !bus.isOffline(ENDPOINT.controller);
      bus.setOffline(ENDPOINT.controller, value);
      return value;
    },
  };

  return {
    clock, bus, controller, entry1, entry2, exit, kiosk, traffic,
    gates: { [ENDPOINT.entry1]: entry1, [ENDPOINT.entry2]: entry2, [ENDPOINT.exit]: exit },
    step, advance, actions,
    onEvent: (callback) => bus.onEvent(callback),
    setLatency: (seconds) => bus.setLatency(seconds),
    setControllerOffline: (value) => bus.setOffline(ENDPOINT.controller, value),
  };
}
