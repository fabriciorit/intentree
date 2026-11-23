// The page script. Builds the world, runs its virtual clock from the animation frames, draws it,
// and exposes window.demo so a host page can drive the demo and hear what the actors do.

import { createWorld } from '../src/world.js';
import { SCENARIOS, runScenario } from '../src/scenarios.js';
import { PHONE_A } from '../src/phone-a.js';
import { PHONE_B } from '../src/phone-b.js';
import { createPhoneView } from './phone-view.js';
import { createWireView } from './wire-view.js';
import { createLogView } from './log-view.js';
import { createControls } from './controls.js';

const DEFAULT_LATENCY_MS = 500;
const MAX_FRAME_MS = 100; // a background tab must not make the world jump ahead

let world = null;
let speed = 1;
const listeners = [];

function emit(event) {
  for (const listener of listeners) listener(event);
  // a state change is also an event of the state machine itself
  if (event.kind === 'state' && event.detail.machine) {
    for (const listener of listeners) listener({ ...event, entity: event.detail.machine });
  }
}

function boot() {
  world = createWorld({ latencyMs: DEFAULT_LATENCY_MS });
  world.subscribe((event) => {
    wire.onEvent(world, event);
    emit(event);
  });
  for (const view of Object.values(phoneViews)) view.reset();
  wire.reset();
  log.reset();
  // what happened while the phones connected
  for (const event of world.log) emit(event);
}

const phoneViews = {};
for (const [slot, config, side] of [['slot-a', PHONE_A, 'left'], ['slot-b', PHONE_B, 'right']]) {
  const who = config.tag;
  phoneViews[config.name] = createPhoneView(
    document.getElementById(slot),
    { name: config.name, owner: config.owner, peerOwner: (who === 'A' ? PHONE_B : PHONE_A).owner, side },
    {
      onSend: (text) => actions.sendMessage(who, text),
      onWifi: (on) => (on ? world.control.goOnline(who) : world.control.goOffline(who)),
      onChat: (open) => {
        const phone = world.phones[who];
        const wanted = open ?? !phone.chatOpen;
        if (wanted) world.control.openChat(who);
        else world.control.closeChat(who);
      },
    },
  );
}

const wire = createWireView({
  stage: document.getElementById('stage'),
  linesSvg: document.getElementById('lines'),
  packetsLayer: document.getElementById('packets'),
  nodes: {
    server: document.getElementById('node-server'),
    gateway: document.getElementById('node-gateway'),
    serverSeq: document.getElementById('server-seq'),
    serverKept: document.getElementById('server-kept'),
    gatewayCount: document.getElementById('gateway-count'),
    presence: { [PHONE_A.name]: document.getElementById('presence-a'), [PHONE_B.name]: document.getElementById('presence-b') },
  },
  phoneViews,
  phoneNames: [PHONE_A.name, PHONE_B.name],
});

const log = createLogView(document.getElementById('log-list'), document.getElementById('log-quiet'));

const controls = createControls({
  scenarios: SCENARIOS,
  onScenario: (scenario) => actions.runScenario(scenario.id),
  onLatency: (ms) => world.control.setLatency(ms),
  onLoss: (rate) => world.control.setLoss(rate),
  onSpeed: (value) => actions.setSpeed(value),
  onReset: () => actions.reset(),
});

// What a host page (or the tour) can trigger by name.
const actions = {
  sendMessage(from = 'A', text) {
    actions.setSpeed(speed === 0 ? 1 : speed);
    return world.control.sendMessage(from, text);
  },
  goOffline: () => world.control.goOffline('B'),
  goOnline: () => world.control.goOnline('B'),
  openChatOnB: () => world.control.openChat('B'),
  // a bad network, and two messages from Alice to see what the retries do
  flakyNetwork() {
    world.control.flakyNetwork();
    actions.sendMessage('A');
    world.clock.after(1500, () => actions.sendMessage('A'));
  },
  steadyNetwork: () => world.control.steadyNetwork(),
  reset() {
    boot();
  },
  runScenario(id) {
    boot();
    runScenario(world, SCENARIOS.find((s) => s.id === id));
  },
  setSpeed(value) {
    speed = value;
    controls.showSpeed(value);
  },
};

window.demo = {
  actions,
  onEvent(callback) {
    listeners.push(callback);
    return () => listeners.splice(listeners.indexOf(callback), 1);
  },
  scenarios: SCENARIOS.map((s) => ({ id: s.id, label: s.label })),
  get world() {
    return world;
  },
};

boot();
let last = performance.now();
function frame(now) {
  const dt = Math.min(now - last, MAX_FRAME_MS);
  last = now;
  world.advance(dt * speed);
  for (const config of [PHONE_A, PHONE_B]) {
    phoneViews[config.name].render(world.phones[config.tag], world.clock.now(), !world.network.isOffline(config.name));
  }
  wire.render(world);
  log.render(world);
  controls.sync(world.network.settings(), world.clock.now());
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);
