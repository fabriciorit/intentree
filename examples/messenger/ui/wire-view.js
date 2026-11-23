// Draws the wire between the actors: the lines, the little packets travelling along them (their
// position comes straight from the network's send and arrival times, so latency is visible), the
// crosses where packets die, and the server and gateway cards.

const QUIET = new Set(['ping', 'hello', 'welcome', 'receipt ack', 'status ack']);
const SVG_NS = 'http://www.w3.org/2000/svg';

function labelFor(packet) {
  const body = packet.body;
  switch (packet.type) {
    case 'send': return `msg ${body.message.id}`;
    case 'deliver': return `msg ${body.message.id} #${body.message.seq}`;
    case 'ack': return `stored #${body.seq}`;
    case 'receipt': return body.kind === 'read' ? 'seen it' : 'got it';
    case 'status': return body.state === 'read' ? 'it was read' : 'it arrived';
    case 'push request': return 'push request';
    case 'push': return 'push';
    default: return packet.type;
  }
}

function classFor(packet) {
  if (QUIET.has(packet.type)) return 'packet--quiet';
  if (packet.type === 'send' || packet.type === 'deliver') return 'packet--msg';
  if (packet.type === 'ack' || packet.type === 'status') return 'packet--ack';
  if (packet.type === 'receipt') return 'packet--receipt';
  return 'packet--push';
}

function svg(tag, attrs = {}) {
  const el = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
  return el;
}

export function createWireView({ stage, linesSvg, packetsLayer, nodes, phoneViews, phoneNames }) {
  const narrow = window.matchMedia('(max-width: 1000px)');
  const chips = new Map(); // packet id -> element
  const lines = [];

  // lines between anchors: [from, to, kind]
  const [nameA, nameB] = phoneNames;
  const specs = [
    { id: 'wifi-a', from: [nameA, 'wifi'], to: ['server'], kind: 'wifi', phone: nameA },
    { id: 'wifi-b', from: [nameB, 'wifi'], to: ['server'], kind: 'wifi', phone: nameB },
    { id: 'back', from: ['server'], to: ['push gateway'], kind: 'backbone' },
    { id: 'cell-a', from: ['push gateway'], to: [nameA, 'cell'], kind: 'cell', phone: nameA },
    { id: 'cell-b', from: ['push gateway'], to: [nameB, 'cell'], kind: 'cell', phone: nameB },
  ];
  for (const spec of specs) {
    const line = svg('line', { class: `wire wire--${spec.kind}` });
    const label = svg('text', { class: `wire__label wire__label--${spec.kind}`, 'text-anchor': 'middle' });
    linesSvg.append(line, label);
    lines.push({ ...spec, line, label });
  }

  function anchors() {
    const box = stage.getBoundingClientRect();
    const at = (x, y) => ({ x: x - box.left, y: y - box.top });
    const centerOf = (el) => {
      const r = el.getBoundingClientRect();
      return at(r.left + r.width / 2, r.top + r.height / 2);
    };
    const map = { server: centerOf(nodes.server), 'push gateway': centerOf(nodes.gateway) };
    for (const [name, view] of Object.entries(phoneViews)) {
      const r = view.element.getBoundingClientRect();
      const rightSide = name === nameA;
      if (narrow.matches) {
        const c = view.card.getBoundingClientRect();
        const x = c.left + c.width / 2;
        map[name] = { wifi: at(x - 24, c.bottom + 2), cell: at(x + 24, c.bottom + 2) };
      } else {
        const x = rightSide ? r.right : r.left;
        const wifiY = Math.min(Math.max(map.server.y + box.top, r.top + 40), r.bottom - 40);
        const cellY = Math.min(Math.max(map['push gateway'].y + box.top, r.top + 40), r.bottom - 40);
        map[name] = { wifi: at(x, wifiY), cell: at(x, cellY) };
      }
    }
    return map;
  }

  function point(map, address, channel) {
    const entry = map[address];
    return entry.wifi ? entry[channel === 'cellular' ? 'cell' : 'wifi'] : entry;
  }

  function resolve(map, ref) {
    const [address, side] = ref;
    return side ? map[address][side] : map[address];
  }

  function placeChip(el, from, to, progress, sign) {
    const dx = to.x - from.x;
    const dy = to.y - from.y;
    const length = Math.hypot(dx, dy) || 1;
    const offset = 11 * sign;
    const x = from.x + dx * progress + (-dy / length) * offset;
    const y = from.y + dy * progress + (dx / length) * offset;
    el.style.transform = `translate(${x.toFixed(1)}px, ${y.toFixed(1)}px) translate(-50%, -50%)`;
  }

  function render(world) {
    const map = anchors();
    const settings = world.network.settings();
    const box = stage.getBoundingClientRect();
    linesSvg.setAttribute('viewBox', `0 0 ${box.width} ${box.height}`);

    for (const entry of lines) {
      const a = resolve(map, entry.from);
      const b = resolve(map, entry.to);
      entry.line.setAttribute('x1', a.x);
      entry.line.setAttribute('y1', a.y);
      entry.line.setAttribute('x2', b.x);
      entry.line.setAttribute('y2', b.y);
      const down = entry.kind === 'wifi' && world.network.isOffline(entry.phone);
      entry.line.classList.toggle('is-down', down);
      entry.label.setAttribute('x', (a.x + b.x) / 2);
      entry.label.setAttribute('y', (a.y + b.y) / 2 - 8);
      entry.label.classList.toggle('is-down', down);
      if (entry.kind === 'wifi') {
        const loss = settings.lossRate > 0 ? `, ${Math.round(settings.lossRate * 100)}% lost` : '';
        entry.label.textContent = down ? 'Wi-Fi is off' : `Wi-Fi ${settings.latencyMs} ms${loss}`;
      } else if (entry.kind === 'cell') {
        entry.label.textContent = 'cellular';
      } else {
        entry.label.textContent = '';
      }
    }

    const now = world.clock.now();
    const live = new Set();
    for (const packet of world.network.inFlightPackets()) {
      live.add(packet.id);
      let chip = chips.get(packet.id);
      if (!chip) {
        chip = document.createElement('div');
        chip.className = `packet ${classFor(packet)} packet--${packet.channel}`;
        chip.textContent = QUIET.has(packet.type) ? '' : labelFor(packet);
        chip.title = packet.type;
        packetsLayer.append(chip);
        chips.set(packet.id, chip);
      }
      const from = point(map, packet.from, packet.channel);
      const to = point(map, packet.to, packet.channel);
      const progress = Math.min(1, Math.max(0, (now - packet.sentAt) / Math.max(1, packet.arriveAt - packet.sentAt)));
      placeChip(chip, from, to, progress, packet.from < packet.to ? 1 : -1);
    }
    for (const [id, chip] of chips) {
      if (!live.has(id)) {
        chip.remove();
        chips.delete(id);
      }
    }

    const snapshot = world.server.snapshot();
    const conversation = snapshot.conversations[0];
    nodes.serverSeq.textContent = conversation ? `numbered up to #${conversation.lastSeq}` : 'no conversation yet';
    const kept = conversation ? conversation.stored : 0;
    nodes.serverKept.textContent = kept === 1 ? 'holding 1 message' : `holding ${kept} messages`;
    nodes.presence[nameA].className = `dot dot--${world.server.presenceOf(nameA)}`;
    nodes.presence[nameB].className = `dot dot--${world.server.presenceOf(nameB)}`;
    const nudges = world.gateway.total();
    nodes.gatewayCount.textContent = nudges === 1 ? '1 nudge sent' : `${nudges} nudges sent`;
  }

  // a packet that dies leaves a cross where it stopped
  function onEvent(world, event) {
    if (event.entity === 'network' && event.kind === 'drop') {
      const { packet, reason, at } = event.detail;
      const map = anchors();
      const from = point(map, packet.from, packet.channel);
      const to = point(map, packet.to, packet.channel);
      const ghost = document.createElement('div');
      ghost.className = 'ghost-packet';
      ghost.textContent = reason === 'lost' ? 'lost' : 'no link';
      placeChip(ghost, from, to, at, packet.from < packet.to ? 1 : -1);
      packetsLayer.append(ghost);
      ghost.addEventListener('animationend', () => ghost.remove());
    }
    if (event.kind === 'receive' && event.entity !== 'network' && !QUIET.has(event.detail.type)) {
      const target = event.entity === 'server' ? nodes.server : event.entity === 'push gateway' ? nodes.gateway : phoneViews[event.entity]?.card;
      if (target) {
        target.classList.remove('is-pulse');
        void target.offsetWidth;
        target.classList.add('is-pulse');
      }
    }
  }

  return { render, onEvent, reset() { for (const chip of chips.values()) chip.remove(); chips.clear(); } };
}
