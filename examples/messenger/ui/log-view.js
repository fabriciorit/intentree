// The event log: turns what the actors report into short sentences, newest on top.

const QUIET_SENDS = new Set(['ping', 'hello', 'welcome', 'receipt ack', 'status ack']);
const MAX_ROWS = 160;

export function describe(event) {
  const d = event.detail;
  if (event.entity === 'network') {
    if (event.kind === 'link') return { text: d.up ? `Wi-Fi of ${d.address} switched on` : `Wi-Fi of ${d.address} switched off`, tone: d.up ? 'ok' : 'bad' };
    if (event.kind === 'drop') {
      const why = d.reason === 'lost' ? 'lost on the wire' : 'no link to the phone';
      return { text: `"${d.packet.type}" from ${d.packet.from} to ${d.packet.to}: ${why}`, tone: 'bad', quiet: ['ping', 'hello', 'welcome', 'receipt ack', 'status ack'].includes(d.packet.type) };
    }
    return null;
  }
  switch (event.kind) {
    case 'state':
      if (d.machine === 'message') return { text: `message ${d.id}: ${d.from ?? 'new'} → ${d.to}`, tone: 'state' };
      if (d.machine === 'presence') return { text: `${d.phone} is ${d.to}`, tone: d.to === 'online' ? 'ok' : 'bad' };
      return { text: `connection: ${d.from} → ${d.to}`, tone: d.to === 'online' ? 'ok' : d.to === 'offline' ? 'bad' : 'state' };
    case 'send':
      return { text: `sends "${d.type}"${d.id ? ` (${d.id})` : ''} to ${d.to}`, tone: 'send', quiet: QUIET_SENDS.has(d.type) };
    case 'store':
      return { text: `stores ${d.id} as number ${d.seq}`, tone: 'state' };
    case 'discard':
      return { text: `${d.id} confirmed by the recipient: the server lets it go`, tone: 'state' };
    case 'retry':
      return { text: `no answer for ${d.id}, trying again (attempt ${d.attempt}, waited ${d.delayMs / 1000} s)`, tone: 'bad' };
    case 'notify':
      return { text: 'shows a push notification', tone: 'ok' };
    case 'reject':
      return { text: `ignores a push request from ${d.from}`, tone: 'bad' };
    default:
      return null;
  }
}

function stamp(ms) {
  return `${(ms / 1000).toFixed(1)} s`;
}

export function createLogView(list, quietBox) {
  let shown = 0;

  function render(world) {
    const log = world.log;
    for (; shown < log.length; shown += 1) {
      const event = log[shown];
      const line = describe(event);
      if (!line) continue;
      const row = document.createElement('li');
      row.className = `log__row log__row--${line.tone}${line.quiet ? ' is-quiet' : ''}`;
      const actor = event.entity.replace(' ', '-');
      row.innerHTML = `<time></time><span class="who who--${actor}"></span><span class="what"></span>`;
      row.querySelector('time').textContent = stamp(event.at);
      row.querySelector('.who').textContent = event.entity;
      row.querySelector('.what').textContent = line.text;
      list.prepend(row);
      while (list.children.length > MAX_ROWS) list.lastElementChild.remove();
    }
    list.classList.toggle('show-quiet', quietBox.checked);
  }

  return { render, reset() { list.innerHTML = ''; shown = 0; } };
}
