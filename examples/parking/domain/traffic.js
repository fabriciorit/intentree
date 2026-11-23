// The drivers: cars that arrive, take a ticket, park, visit the kiosk, and leave. This is the
// world around the garage, not part of the garage: a car only touches the gates through their
// physical interface (arrive, take the outcome, sensor under the barrier).

import * as layout from './layout.js';

const SPEED = 50; // drawing units per simulated second
const ENTRY_QUEUE = ['queue', 'request', 'take-ticket', 'await-open'];
const EXIT_QUEUE = ['exit-queue', 'exit-wait', 'denied-wait', 'exit-await-open'];
const SENSED = ['enter', 'leaving'];

export function createTraffic({ entries, exit, kiosk, bus, random = Math.random }) {
  const cars = [];
  const slotOwner = layout.SLOTS.map(() => null);
  const commands = []; // manual requests waiting for a car that can do them
  let nextNumber = 1;
  let queueClock = 0;
  let arrivalsPerMinute = 4;
  let arrivalsPaused = false;
  let obstacle = null; // { barrier, status: 'armed' | 'present', left }

  const barriers = { [entries[0].name]: entries[0].barrier, [entries[1].name]: entries[1].barrier, [exit.name]: exit.barrier };
  const sensorX = { [entries[0].name]: layout.LANE_X[0], [entries[1].name]: layout.LANE_X[1], [exit.name]: layout.EXIT_X };

  const say = (text, car) => bus.emit('traffic', 'car', { text, car: car && car.id });

  function spawn(lane, { mode = 'auto', dwell } = {}) {
    const number = nextNumber++;
    const car = {
      id: `car ${number}`, number, lane, mode, phase: 'queue',
      x: layout.LANE_X[lane], y: layout.OFFSCREEN_Y - 40, heading: 0,
      route: [], leg: 0, timer: 0, ticket: null, slot: null, paid: false, afterPay: 'exit',
      dwell: dwell ?? 25 + random() * 80, note: '', hue: Math.floor(random() * 360),
    };
    cars.push(car);
    say(`${car.id} arrives at entry ${lane + 1}`, car);
    return car.id;
  }

  // --- movement
  function turnTo(car, dx, dy, dt) {
    if (dx === 0 && dy === 0) return;
    const wanted = (Math.atan2(dx, -dy) * 180) / Math.PI;
    let delta = ((wanted - car.heading + 540) % 360) - 180;
    delta *= Math.min(1, dt * 10);
    car.heading += delta;
  }

  function moveToward(car, target, dt) {
    const dx = target.x - car.x;
    const dy = target.y - car.y;
    const distance = Math.hypot(dx, dy);
    const step = SPEED * dt;
    turnTo(car, dx, dy, dt);
    if (distance <= step) {
      car.x = target.x;
      car.y = target.y;
      return true;
    }
    car.x += (dx / distance) * step;
    car.y += (dy / distance) * step;
    return false;
  }

  function setRoute(car, route) {
    car.route = route;
    car.leg = 0;
  }

  function follow(car, dt) {
    let budget = dt;
    while (car.leg < car.route.length) {
      const before = { x: car.x, y: car.y };
      if (!moveToward(car, car.route[car.leg], budget)) return false;
      const used = Math.hypot(car.x - before.x, car.y - before.y) / SPEED;
      budget = Math.max(0, budget - used);
      car.leg += 1;
      if (budget <= 0 && car.leg < car.route.length) return false;
    }
    return true;
  }

  const queueOf = (car) => cars.filter((c) => (c.lane === car.lane && ENTRY_QUEUE.includes(c.phase)));
  const exitQueue = () => cars.filter((c) => EXIT_QUEUE.includes(c.phase)).sort((a, b) => a.queuedAt - b.queuedAt);
  const entryTarget = (car, index) => ({ x: layout.LANE_X[car.lane], y: layout.HEAD_Y + index * layout.QUEUE_GAP });
  const exitTarget = (index) => ({ x: layout.EXIT_X, y: layout.EXIT_HEAD_Y - index * layout.QUEUE_GAP });

  function takeSlot(car) {
    const index = slotOwner.findIndex((owner) => owner === null);
    if (index < 0) return false;
    slotOwner[index] = car.id;
    car.slot = layout.SLOTS[index];
    return true;
  }

  function release(car) {
    const index = slotOwner.indexOf(car.id);
    if (index >= 0) slotOwner[index] = null;
  }

  // --- one car, one step
  function stepCar(car, dt) {
    const gate = entries[car.lane];
    switch (car.phase) {
      case 'queue': {
        const queue = queueOf(car);
        const index = queue.indexOf(car);
        const arrived = moveToward(car, entryTarget(car, index), dt);
        if (arrived && index === 0 && gate.carArrived(car.id)) car.phase = 'request';
        break;
      }
      case 'request': {
        const outcome = gate.takeOutcome(car.id);
        if (!outcome) break;
        if (outcome.ticketId) {
          car.ticket = outcome.ticketId;
          car.timer = 0.6;
          car.phase = 'take-ticket';
          say(`${car.id} takes ticket ${car.ticket}`, car);
        } else {
          car.note = outcome.full ? 'garage full' : 'no contact';
          setRoute(car, [{ x: car.x - 50, y: car.y }, { x: car.x - 50, y: layout.OFFSCREEN_Y }]);
          car.phase = 'turnaway';
          say(`${car.id} is turned away: ${car.note}`, car);
        }
        break;
      }
      case 'take-ticket':
        car.timer -= dt;
        if (car.timer <= 0) car.phase = 'await-open';
        break;
      case 'await-open':
        if (gate.barrier.state === 'open') {
          if (takeSlot(car)) {
            setRoute(car, layout.entryRoute(car.lane, car.slot));
            car.phase = 'enter';
          }
        }
        break;
      case 'enter':
        if (follow(car, dt)) {
          car.phase = 'parked';
          car.timer = 0;
          say(`${car.id} parks in ${car.slot.id}`, car);
        }
        break;
      case 'parked':
        car.timer += dt;
        if (car.mode === 'auto' && car.timer >= car.dwell) {
          if (!car.paid) goToKiosk(car, 'exit');
          else goToExit(car);
        }
        break;
      case 'to-kiosk':
        if (follow(car, dt)) {
          if (kiosk.insertTicket(car.id, car.ticket)) car.phase = 'paying';
        }
        break;
      case 'paying': {
        const outcome = kiosk.takeOutcome(car.id);
        if (!outcome) break;
        car.note = outcome.ok ? '' : outcome.reason;
        if (outcome.ok || outcome.reason === 'already paid') car.paid = true;
        car.timer = 0.8;
        car.phase = 'paid-wait';
        say(outcome.ok ? `${car.id} pays ${outcome.amount.toFixed(2)} for ${car.ticket}` : `${car.id} payment refused: ${outcome.reason}`, car);
        break;
      }
      case 'paid-wait':
        car.timer -= dt;
        if (car.timer <= 0) {
          kiosk.reset();
          if (car.paid && car.afterPay === 'exit') {
            setRoute(car, layout.kioskToExit());
            car.phase = 'to-exit';
          } else {
            setRoute(car, layout.reverseRoute(layout.slotToKiosk(car.slot)));
            car.phase = 'to-slot';
          }
        }
        break;
      case 'to-slot':
        if (follow(car, dt)) {
          car.phase = 'parked';
          car.timer = car.dwell; // already waited: auto cars act at once
        }
        break;
      case 'to-exit':
        if (follow(car, dt)) {
          car.queuedAt = queueClock++;
          car.phase = 'exit-queue';
        }
        break;
      case 'exit-queue': {
        const queue = exitQueue();
        const index = queue.indexOf(car);
        const arrived = moveToward(car, exitTarget(index), dt);
        if (arrived && index === 0 && exit.readTicket(car.id, car.ticket)) {
          car.phase = 'exit-wait';
          say(`${car.id} shows ${car.ticket} at the exit`, car);
        }
        break;
      }
      case 'exit-wait': {
        const outcome = exit.takeOutcome(car.id);
        if (!outcome) break;
        if (outcome.ok) {
          car.phase = 'exit-await-open';
        } else {
          car.note = outcome.denied;
          if (outcome.denied === 'unpaid' || outcome.denied === 'expired') car.paid = false;
          car.timer = 2.2;
          car.phase = 'denied-wait';
          say(`${car.id} is refused at the exit: ${outcome.denied}`, car);
        }
        break;
      }
      case 'denied-wait':
        car.timer -= dt;
        if (car.timer <= 0) {
          exit.reset();
          setRoute(car, layout.exitToSlot(car.slot));
          car.phase = 'to-slot';
          car.mode = 'manual';
        }
        break;
      case 'exit-await-open':
        if (exit.barrier.state === 'open') {
          setRoute(car, layout.passExit());
          car.phase = 'leaving';
        }
        break;
      case 'leaving':
        if (follow(car, dt)) {
          release(car);
          exit.reset();
          say(`${car.id} leaves the garage`, car);
          cars.splice(cars.indexOf(car), 1);
        }
        break;
      case 'turnaway':
        if (follow(car, dt)) cars.splice(cars.indexOf(car), 1);
        break;
      default:
        break;
    }
  }

  function goToKiosk(car, afterPay) {
    car.afterPay = afterPay;
    setRoute(car, layout.slotToKiosk(car.slot));
    car.phase = 'to-kiosk';
  }

  function goToExit(car) {
    setRoute(car, layout.slotToExit(car.slot));
    car.phase = 'to-exit';
  }

  // --- manual requests: served by the first car that can do them (the user's own cars first)
  function candidate(test) {
    const parked = cars.filter((c) => c.phase === 'parked' && test(c));
    return parked.find((c) => c.mode === 'manual') || parked[0] || null;
  }

  function serveCommands() {
    for (let i = 0; i < commands.length; i++) {
      const wanted = commands[i];
      let car = null;
      if (wanted === 'pay') car = candidate((c) => !c.paid);
      else if (wanted === 'leave') car = candidate((c) => c.paid);
      else if (wanted === 'tryLeaveUnpaid') car = candidate((c) => !c.paid);
      if (!car) continue;
      commands.splice(i, 1);
      i -= 1;
      car.mode = 'manual';
      if (wanted === 'pay') {
        goToKiosk(car, 'slot');
        say(`${car.id} walks to the kiosk with ${car.ticket}`, car);
      } else {
        goToExit(car);
        say(`${car.id} heads for the exit${wanted === 'tryLeaveUnpaid' ? ' without paying' : ''}`, car);
      }
    }
  }

  // --- sensors and the obstacle
  function updateSensors(dt) {
    if (obstacle) {
      if (obstacle.status === 'armed') {
        const closing = Object.keys(barriers).find((n) => barriers[n].state === 'closing');
        if (closing) {
          obstacle = { barrier: closing, status: 'present', left: 4 };
          say(`someone steps under the arm of ${closing}`);
        }
      } else if (obstacle.status === 'present') {
        obstacle.left -= dt;
        if (obstacle.left <= 0) obstacle = null;
      }
    }
    for (const [name, barrier] of Object.entries(barriers)) {
      const x = sensorX[name];
      const car = cars.some((c) => SENSED.includes(c.phase) && Math.abs(c.x - x) < layout.ZONE.half
        && c.y > layout.ZONE.top && c.y < layout.ZONE.bottom);
      const person = obstacle && obstacle.status === 'present' && obstacle.barrier === name;
      barrier.setSensor(car || Boolean(person));
    }
  }

  function autoArrivals(dt) {
    if (arrivalsPaused || arrivalsPerMinute <= 0) return;
    if (random() < (arrivalsPerMinute / 60) * dt) {
      const lane = random() < 0.5 ? 0 : 1;
      if (cars.filter((c) => c.lane === lane && ENTRY_QUEUE.includes(c.phase)).length < 5) spawn(lane);
    }
  }

  function step(dt) {
    autoArrivals(dt);
    serveCommands();
    for (const car of [...cars]) stepCar(car, dt);
    updateSensors(dt);
  }

  // A pedestrian will step under the first barrier that starts to close.
  function armObstacle() {
    obstacle = { barrier: null, status: 'armed', left: 0 };
    return Object.values(barriers).some((b) => b.state !== 'closed');
  }

  return {
    spawn,
    step,
    request: (wanted) => commands.push(wanted),
    armObstacle,
    setArrivalsPerMinute: (value) => { arrivalsPerMinute = value; },
    getArrivalsPerMinute: () => arrivalsPerMinute,
    pauseArrivals: (value) => { arrivalsPaused = value; },
    queued: () => cars.filter((c) => ENTRY_QUEUE.includes(c.phase)).length,
    get cars() { return cars; },
    get obstacle() { return obstacle; },
    pendingCommands: () => commands.length,
    slotOwners: () => [...slotOwner],
  };
}
