// A barrier with its Barrier state machine and a sensor under the arm.
// closed -> opening -> open -> closing -> closed; closing -> blocked when the sensor sees a car
// (the arm goes back up and waits, it never closes on a car); blocked -> closing once clear.

export const BARRIER_STATES = ['closed', 'opening', 'open', 'closing', 'blocked'];
export const BARRIER_TRANSITIONS = [
  ['closed', 'opening'],
  ['opening', 'open'],
  ['open', 'closing'],
  ['closing', 'closed'],
  ['closing', 'opening'],
  ['closing', 'blocked'],
  ['blocked', 'closing'],
  ['blocked', 'opening'],
];

export const TIMING = Object.freeze({ open: 1.5, close: 2.5, hold: 6, afterCar: 1.5 });

export function createBarrier({ onChange = () => {} } = {}) {
  let state = 'closed';
  let openness = 0; // 0 = arm down across the lane, 1 = arm fully up
  let hold = 0;
  let sensor = false;

  function move(to) {
    const legal = BARRIER_TRANSITIONS.some(([from, next]) => from === state && next === to);
    if (!legal) throw new Error(`barrier: illegal move ${state} -> ${to}`);
    const from = state;
    state = to;
    onChange(from, to);
  }

  // Called only by a gate that was authorized (a ticket issued, an exit granted).
  function open() {
    if (state === 'closed' || state === 'closing' || state === 'blocked') move('opening');
    else if (state === 'open') hold = TIMING.hold;
  }

  function setSensor(value) {
    sensor = value;
    if (sensor && state === 'closing') move('blocked');
  }

  function tick(dt) {
    if (state === 'opening') {
      openness = Math.min(1, openness + dt / TIMING.open);
      if (openness >= 1) {
        move('open');
        hold = TIMING.hold;
      }
    } else if (state === 'open') {
      if (sensor) hold = Math.max(hold, TIMING.afterCar);
      else {
        hold -= dt;
        if (hold <= 0) move('closing');
      }
    } else if (state === 'closing') {
      if (sensor) move('blocked');
      else {
        openness = Math.max(0, openness - dt / TIMING.close);
        if (openness <= 0) move('closed');
      }
    } else if (state === 'blocked') {
      openness = Math.min(1, openness + dt / TIMING.open); // back up, out of the car's way
      if (!sensor && openness >= 1) move('closing');
    }
  }

  return {
    open,
    setSensor,
    tick,
    get state() { return state; },
    get openness() { return openness; },
    get sensor() { return sensor; },
  };
}
