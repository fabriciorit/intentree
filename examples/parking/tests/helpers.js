import { createClock } from '../domain/clock.js';
import { createBus } from '../domain/bus.js';
import { createController } from '../domain/controller.js';
import { createEntryGate1 } from '../domain/entry-gate-1.js';
import { createEntryGate2 } from '../domain/entry-gate-2.js';
import { createExitGate } from '../domain/exit-gate.js';
import { createKiosk } from '../domain/kiosk.js';

// A garage without cars: the actors and the bus, driven by hand.
export function actors({ capacity = 3 } = {}) {
  const clock = createClock(0);
  const bus = createBus({ clock });
  const controller = createController({ bus, clock, capacity });
  const entry1 = createEntryGate1({ bus });
  const entry2 = createEntryGate2({ bus });
  const exit = createExitGate({ bus });
  const kiosk = createKiosk({ bus });
  const events = [];
  bus.onEvent((e) => events.push(e));
  controller.announce();
  bus.deliver();
  const run = (seconds, dt = 0.25) => {
    for (let t = 0; t < seconds; t += dt) {
      clock.advance(dt);
      bus.deliver();
      for (const a of [entry1, entry2, exit, kiosk]) a.tick(dt);
      controller.sweep();
      bus.deliver();
    }
  };
  return { clock, bus, controller, entry1, entry2, exit, kiosk, events, run };
}

export function seeded(seed = 7) {
  let s = seed;
  return () => {
    s = (s * 16807) % 2147483647;
    return s / 2147483647;
  };
}
