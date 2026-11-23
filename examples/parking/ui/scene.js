// The garage seen from above, in SVG: static drawing once, live parts (cars, barrier arms,
// signs, screens) updated every frame from the garage state. No rules here.

import * as layout from '../domain/layout.js';

const NS = 'http://www.w3.org/2000/svg';

function el(tag, attrs, parent) {
  const node = document.createElementNS(NS, tag);
  for (const [key, value] of Object.entries(attrs || {})) node.setAttribute(key, value);
  if (parent) parent.appendChild(node);
  return node;
}

function text(parent, content, attrs) {
  const node = el('text', attrs, parent);
  node.textContent = content;
  return node;
}

export function createScene(svg, garage) {
  const { width, height } = layout.VIEW;
  svg.setAttribute('viewBox', `0 0 ${width} ${height}`);
  const live = {};

  // --- static drawing
  const ground = el('g', { class: 'ground' }, svg);
  el('rect', { x: 0, y: layout.WALL_Y, width, height: height - layout.WALL_Y, class: 'street' }, ground);
  el('rect', { x: 10, y: 10, width: width - 20, height: layout.WALL_Y - 10, rx: 18, class: 'floor' }, ground);
  const roads = el('g', { class: 'road' }, ground);
  const road = (x, y, w, h) => el('rect', { x, y, width: w, height: h, rx: 4 }, roads);
  road(layout.CONNECTOR_LEFT - 22, layout.AISLE_HIGH - 22, layout.CONNECTOR_RIGHT - layout.CONNECTOR_LEFT + 44, 44);
  road(60, layout.AISLE_LOW - 22, layout.EXIT_X - 60 + 30, 44);
  road(layout.CONNECTOR_LEFT - 22, layout.AISLE_HIGH - 22, 44, layout.AISLE_LOW - layout.AISLE_HIGH + 44);
  road(layout.CONNECTOR_RIGHT - 22, layout.AISLE_HIGH - 22, 44, layout.AISLE_LOW - layout.AISLE_HIGH + 44);
  for (const x of layout.LANE_X) road(x - 34, layout.AISLE_LOW, 68, height - layout.AISLE_LOW);
  road(layout.EXIT_X - 30, layout.AISLE_LOW - 22, 60, height - layout.AISLE_LOW + 22);

  const wall = el('g', { class: 'wall' }, ground);
  const gaps = [...layout.LANE_X.map((x) => [x - 38, x + 38]), [layout.EXIT_X - 38, layout.EXIT_X + 38]].sort((a, b) => a[0] - b[0]);
  let from = 10;
  for (const [start, end] of gaps) {
    el('rect', { x: from, y: layout.WALL_Y - 5, width: start - from, height: 10, rx: 3 }, wall);
    from = end;
  }
  el('rect', { x: from, y: layout.WALL_Y - 5, width: width - 10 - from, height: 10, rx: 3 }, wall);

  const slots = el('g', { class: 'slots' }, ground);
  live.slots = layout.SLOTS.map((slot) => {
    const g = el('g', {}, slots);
    el('rect', { x: slot.x - 38, y: slot.y - 28, width: 76, height: 56, rx: 6, class: 'slot' }, g);
    text(g, slot.id, { x: slot.x, y: slot.row === 'A' ? slot.y - 17 : slot.y + 23, 'text-anchor': 'middle', class: 'slot-label' });
    return g;
  });

  // kiosk
  const k = layout.KIOSK_BOX;
  const kiosk = el('g', { class: 'kiosk' }, svg);
  el('rect', { x: k.x, y: k.y, width: k.width, height: k.height, rx: 8, class: 'device' }, kiosk);
  text(kiosk, 'PAYMENT KIOSK', { x: k.x + k.width / 2, y: k.y + 12, 'text-anchor': 'middle', class: 'device-title' });
  live.kioskScreen = text(kiosk, '', { x: k.x + k.width / 2, y: k.y + 28, 'text-anchor': 'middle', class: 'screen' });

  // entry signs, barriers, printers
  const gates = [
    { gate: garage.entry1, x: layout.LANE_X[0], label: 'ENTRY 1', labelAt: [24, 'start'], entry: true },
    { gate: garage.entry2, x: layout.LANE_X[1], label: 'ENTRY 2', labelAt: [layout.LANE_X[1] + 46, 'start'], entry: true },
    { gate: garage.exit, x: layout.EXIT_X, label: 'EXIT', labelAt: [layout.EXIT_X - 46, 'end'], entry: false },
  ];
  live.gates = gates.map((g) => {
    const group = el('g', { class: 'gate' }, svg);
    const ui = { gate: g.gate };
    if (g.entry) {
      const sign = el('g', { class: 'sign' }, group);
      el('rect', { x: g.x - 54, y: 366, width: 108, height: 50, rx: 10 }, sign);
      text(sign, 'FREE', { x: g.x, y: 380, 'text-anchor': 'middle', class: 'sign-caption' });
      ui.sign = sign;
      ui.signText = text(sign, '', { x: g.x, y: 405, 'text-anchor': 'middle', class: 'sign-text' });
      const printer = el('g', { class: 'printer' }, group);
      el('rect', { x: g.x + 40, y: 450, width: 44, height: 26, rx: 6, class: 'device' }, printer);
      text(printer, 'ticket', { x: g.x + 62, y: 461, 'text-anchor': 'middle', class: 'device-title' });
      ui.printerText = text(printer, '', { x: g.x + 62, y: 472, 'text-anchor': 'middle', class: 'screen' });
    } else {
      const reader = el('g', { class: 'printer' }, group);
      el('rect', { x: g.x - 128, y: 450, width: 94, height: 26, rx: 6, class: 'device' }, reader);
      text(reader, 'ticket reader', { x: g.x - 81, y: 461, 'text-anchor': 'middle', class: 'device-title' });
      ui.readerText = text(reader, '', { x: g.x - 81, y: 472, 'text-anchor': 'middle', class: 'screen' });
    }
    ui.zone = el('rect', { x: g.x - layout.ZONE.half, y: layout.ZONE.top, width: layout.ZONE.half * 2, height: layout.ZONE.bottom - layout.ZONE.top, rx: 6, class: 'zone' }, group);
    el('rect', { x: g.x - 52, y: layout.BARRIER_Y - 11, width: 14, height: 22, rx: 3, class: 'booth' }, group);
    ui.armX = g.x - 38;
    ui.arm = el('g', { class: 'arm', transform: `translate(${ui.armX} ${layout.BARRIER_Y})` }, group);
    el('rect', { x: 0, y: -3.5, width: 76, height: 7, rx: 3.5 }, ui.arm);
    for (let i = 0; i < 4; i++) el('rect', { x: 10 + i * 17, y: -3.5, width: 8, height: 7, class: 'stripe' }, ui.arm);
    ui.state = text(group, '', { x: g.labelAt[0], y: 528, 'text-anchor': g.labelAt[1], class: 'gate-state' });
    text(group, g.label, { x: g.labelAt[0], y: 512, 'text-anchor': g.labelAt[1], class: 'gate-label' });
    return ui;
  });

  const carsLayer = el('g', { class: 'cars' }, svg);
  const people = el('g', { class: 'people' }, svg);
  live.person = el('g', { class: 'person', visibility: 'hidden' }, people);
  el('circle', { r: 11 }, live.person);
  text(live.person, '!', { y: 5, 'text-anchor': 'middle' });
  const carNodes = new Map();

  function carNode(car) {
    let node = carNodes.get(car.id);
    if (node) return node;
    const group = el('g', { class: 'car' }, carsLayer);
    const body = el('g', {}, group);
    el('rect', { x: -11, y: -20, width: 22, height: 40, rx: 7, class: 'car-body', fill: `hsl(${car.hue} 62% 56%)` }, body);
    el('rect', { x: -8, y: -12, width: 16, height: 8, rx: 2, class: 'car-glass' }, body);
    el('rect', { x: -8, y: 8, width: 16, height: 5, rx: 2, class: 'car-glass' }, body);
    const badge = el('g', { class: 'car-badge' }, group);
    el('circle', { cx: 12, cy: -16, r: 5 }, badge);
    const label = text(group, '', { y: 33, 'text-anchor': 'middle', class: 'car-label' });
    node = { group, body, badge, label };
    carNodes.set(car.id, node);
    return node;
  }

  // --- per frame
  const owners = () => garage.traffic.slotOwners();

  function render() {
    for (const ui of live.gates) {
      const { gate } = ui;
      ui.arm.setAttribute('transform', `translate(${ui.armX} ${layout.BARRIER_Y}) rotate(${-90 * gate.barrier.openness})`);
      ui.arm.dataset.state = gate.barrier.state;
      ui.zone.classList.toggle('occupied', gate.barrier.sensor);
      ui.state.textContent = gate.barrier.state;
      ui.state.dataset.state = gate.barrier.state;
      if (ui.sign) {
        ui.signText.textContent = gate.display.free === null ? '--' : gate.display.free === 0 ? 'FULL' : `${gate.display.free} / ${gate.display.capacity}`;
        ui.sign.classList.toggle('full', gate.display.free === 0);
        ui.sign.classList.toggle('low', gate.display.free !== null && gate.display.free > 0 && gate.display.free <= 3);
        ui.printerText.textContent = gate.printer.last || '';
      } else {
        ui.readerText.textContent = gate.screen.text;
        ui.readerText.classList.toggle('alert', /PAY|EXPIRED|USED|UNKNOWN|NO CONTACT/.test(gate.screen.text));
      }
    }
    const screen = garage.kiosk.screen;
    live.kioskScreen.textContent = screen.amount ? `PAID ${screen.amount.toFixed(2)}` : screen.text;
    live.kioskScreen.classList.toggle('alert', /REFUSED|NO CONTACT/.test(screen.text));

    const taken = owners();
    live.slots.forEach((g, i) => g.classList.toggle('taken', taken[i] !== null));

    const seen = new Set();
    for (const car of garage.traffic.cars) {
      seen.add(car.id);
      const node = carNode(car);
      node.group.setAttribute('transform', `translate(${car.x.toFixed(1)} ${car.y.toFixed(1)})`);
      node.body.setAttribute('transform', `rotate(${car.heading.toFixed(1)})`);
      node.group.classList.toggle('manual', car.mode === 'manual');
      node.badge.classList.toggle('paid', car.paid);
      const problem = ['denied-wait', 'turnaway'].includes(car.phase) || (car.note && car.phase === 'paid-wait');
      node.group.classList.toggle('problem', problem);
      node.label.textContent = problem ? car.note : car.ticket || '';
    }
    for (const [id, node] of carNodes) {
      if (!seen.has(id)) {
        node.group.remove();
        carNodes.delete(id);
      }
    }

    const obstacle = garage.traffic.obstacle;
    if (obstacle && obstacle.status === 'present') {
      const x = obstacle.barrier === garage.exit.name ? layout.EXIT_X : garage.entry1.name === obstacle.barrier ? layout.LANE_X[0] : layout.LANE_X[1];
      live.person.setAttribute('transform', `translate(${x} ${layout.BARRIER_Y + 3})`);
      live.person.setAttribute('visibility', 'visible');
    } else live.person.setAttribute('visibility', 'hidden');
  }

  return { render };
}
