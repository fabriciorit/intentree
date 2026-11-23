// Draws a state machine as a small SVG diagram, from the same state and transition lists the
// domain code enforces, and lights up the current states live.

const NS = 'http://www.w3.org/2000/svg';
const BOX = { w: 86, h: 28 };

function el(tag, attrs, parent) {
  const node = document.createElementNS(NS, tag);
  for (const [key, value] of Object.entries(attrs || {})) node.setAttribute(key, value);
  if (parent) parent.appendChild(node);
  return node;
}

// Where the line from one box center to another leaves the first box.
function edgeEnd(from, to) {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const scale = Math.min(
    dx === 0 ? Infinity : BOX.w / 2 / Math.abs(dx),
    dy === 0 ? Infinity : BOX.h / 2 / Math.abs(dy),
  );
  return { x: from.x + dx * scale, y: from.y + dy * scale };
}

export function createMachineDiagram(container, { name, states, transitions, positions, width, height }) {
  const svg = el('svg', { viewBox: `0 0 ${width} ${height}`, class: 'machine', role: 'img', 'aria-label': `${name} state machine` });
  const id = `arrow-${name}`;
  const defs = el('defs', {}, svg);
  const marker = el('marker', { id, viewBox: '0 0 10 10', refX: 9, refY: 5, markerWidth: 7, markerHeight: 7, orient: 'auto-start-reverse' }, defs);
  el('path', { d: 'M0 0 L10 5 L0 10 z', class: 'arrow-head' }, marker);

  const pairs = new Set(transitions.map(([a, b]) => `${a}>${b}`));
  for (const [a, b] of transitions) {
    if (a === b) {
      const p = positions[a];
      const d = `M ${p.x - 12} ${p.y + BOX.h / 2} C ${p.x - 18} ${p.y + BOX.h / 2 + 26}, ${p.x + 18} ${p.y + BOX.h / 2 + 26}, ${p.x + 12} ${p.y + BOX.h / 2 + 2}`;
      el('path', { d, class: 'edge', fill: 'none', 'marker-end': `url(#${id})` }, svg);
      continue;
    }
    const from = positions[a];
    const to = positions[b];
    const reciprocal = pairs.has(`${b}>${a}`);
    const length = Math.hypot(to.x - from.x, to.y - from.y);
    const nx = (-(to.y - from.y) / length) * (reciprocal ? 6 : 0);
    const ny = ((to.x - from.x) / length) * (reciprocal ? 6 : 0);
    const start = edgeEnd({ x: from.x + nx, y: from.y + ny }, { x: to.x + nx, y: to.y + ny });
    const end = edgeEnd({ x: to.x + nx, y: to.y + ny }, { x: from.x + nx, y: from.y + ny });
    el('line', { x1: start.x, y1: start.y, x2: end.x, y2: end.y, class: 'edge', 'marker-end': `url(#${id})` }, svg);
  }

  const boxes = {};
  for (const state of states) {
    const p = positions[state];
    const group = el('g', { class: 'state', transform: `translate(${p.x} ${p.y})` }, svg);
    el('rect', { x: -BOX.w / 2, y: -BOX.h / 2, width: BOX.w, height: BOX.h, rx: 9 }, group);
    const label = el('text', { y: 4, 'text-anchor': 'middle', class: 'state-name' }, group);
    label.textContent = state;
    const chips = el('g', { class: 'chips', transform: `translate(${BOX.w / 2 - 2} ${-BOX.h / 2 - 12})` }, group);
    boxes[state] = { group, label, chips, text: state };
  }
  container.appendChild(svg);

  // info: state -> list of chip labels (e.g. ["E1", "X"] or ["3"])
  function update(info) {
    for (const state of states) {
      const chips = info[state] || [];
      const box = boxes[state];
      box.group.classList.toggle('active', chips.length > 0);
      const key = chips.join('|');
      if (box.key === key) continue;
      box.key = key;
      box.chips.replaceChildren();
      let offset = 0;
      for (const chip of [...chips].reverse()) {
        const width = 12 + chip.length * 6;
        offset -= width;
        const g = el('g', { transform: `translate(${offset} 0)` }, box.chips);
        el('rect', { width, height: 15, rx: 7.5 }, g);
        const t = el('text', { x: width / 2, y: 11, 'text-anchor': 'middle' }, g);
        t.textContent = chip;
        offset -= 3;
      }
    }
  }

  return { update };
}
