// The page: runs the garage in real time, draws it, wires the controls and exposes
// window.demo for the tour.

import { createGarage } from '../domain/garage.js';
import { formatClock } from '../domain/clock.js';
import { createScene } from './scene.js';
import { createPanels } from './panels.js';

const garage = createGarage();
const $ = (id) => document.getElementById(id);

const scene = createScene($('scene'), garage);
const panels = createPanels({
  garage,
  ticketList: $('tickets'),
  ticketSummary: $('ticket-summary'),
  machines: { ticket: $('machine-ticket'), barrier: $('machine-barrier') },
  logList: $('log'),
});

// --- time
let speed = 4;
let paused = false;
const speeds = [1, 4, 16, 64];

function setSpeed(value) {
  speed = value;
  paused = false;
  for (const button of document.querySelectorAll('[data-speed]')) {
    button.setAttribute('aria-pressed', String(!paused && Number(button.dataset.speed) === speed));
  }
  $('pause').setAttribute('aria-pressed', 'false');
}
for (const value of speeds) {
  const button = document.querySelector(`[data-speed="${value}"]`);
  button.addEventListener('click', () => setSpeed(value));
}
$('pause').addEventListener('click', () => {
  paused = !paused;
  $('pause').setAttribute('aria-pressed', String(paused));
  for (const button of document.querySelectorAll('[data-speed]')) {
    button.setAttribute('aria-pressed', String(!paused && Number(button.dataset.speed) === speed));
  }
});
setSpeed(speed);

let last = performance.now();
function frame(now) {
  const dt = Math.min(0.1, (now - last) / 1000);
  last = now;
  if (!paused) garage.advance(dt * speed);
  scene.render();
  panels.render();
  $('clock').textContent = formatClock(garage.clock.now());
  requestAnimationFrame(frame);
}

// --- controls
const actions = garage.actions;
const press = (id, action) => $(id).addEventListener('click', () => action());
press('rush', () => actions.rushHour());
press('arrive1', () => actions.arriveAtEntry1());
press('arrive2', () => actions.arriveAtEntry2());
press('pay', () => actions.payTicket());
press('leave', () => actions.leave());
press('try-leave', () => actions.tryLeaveUnpaid());
press('grace', () => actions.waitPastGrace());
press('obstacle', () => actions.obstacleUnderBarrier());

function bindSlider(id, output, apply, format) {
  const input = $(id);
  const sync = () => {
    apply(Number(input.value));
    $(output).textContent = format(Number(input.value));
  };
  input.addEventListener('input', sync);
  sync();
}
bindSlider('rate', 'rate-out', (v) => garage.traffic.setArrivalsPerMinute(v), (v) => (v === 0 ? 'none' : `${v} / min`));
bindSlider('latency', 'latency-out', (v) => garage.setLatency(v), (v) => (v === 0 ? 'instant' : `${v} s`));
$('offline').addEventListener('change', (e) => garage.setControllerOffline(e.target.checked));

// --- the hook for the page shell and the tour
const listeners = [];
garage.onEvent((e) => listeners.forEach((cb) => cb(e)));
window.demo = {
  actions: {
    ...actions,
    toggleControllerOffline() {
      const value = actions.toggleControllerOffline();
      $('offline').checked = value;
      return value;
    },
  },
  onEvent: (callback) => { listeners.push(callback); },
  garage,
};

requestAnimationFrame(frame);
