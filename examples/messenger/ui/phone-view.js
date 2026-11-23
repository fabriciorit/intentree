// Draws one phone: status bar, push notification, chat list, chat screen with bubbles and
// ticks. It only reads a phone's public state; it never changes the system by itself.

import { tickFor } from '../src/message-states.js';

const TICKS_SVG = `
<svg class="ticks__svg" viewBox="0 0 20 12" aria-hidden="true">
  <g class="t-clock"><circle cx="7" cy="6" r="4.6"/><path d="M7 3.4V6l1.8 1.2"/></g>
  <g class="t-first"><path d="M1.5 6.6l3.4 3.4L11.6 2.2"/></g>
  <g class="t-second"><path d="M7.4 9.7l.6.4 6.6-7.9"/></g>
</svg>`;

const TICK_LABELS = {
  draft: 'being written',
  clock: 'waiting in the outbox',
  'one-grey': 'stored on the server',
  'two-grey': 'delivered to the other phone',
  'two-blue': 'read',
};

const CONNECTION_TEXT = { online: 'online', syncing: 'syncing', offline: 'offline' };

export function formatTime(ms) {
  const total = Math.floor(ms / 1000);
  return `${String(Math.floor(total / 60)).padStart(2, '0')}:${String(total % 60).padStart(2, '0')}`;
}

export function createPhoneView(root, { name, owner, peerOwner, side }, { onSend, onWifi, onChat }) {
  root.innerHTML = `
    <div class="phone-card phone-card--${side}">
      <div class="phone-card__label"><b>${name}</b><span>${owner}'s phone</span></div>
      <div class="phone">
        <div class="phone__screen">
          <div class="statusbar">
            <span class="statusbar__time"></span>
            <span class="statusbar__link"><svg class="wifi" viewBox="0 0 20 16" aria-hidden="true"><path d="M2 6.2a11 11 0 0 1 16 0M5 9.4a6.6 6.6 0 0 1 10 0M8 12.5a2.4 2.4 0 0 1 4 0"/><path class="wifi__cut" d="M3 14L17 2"/></svg><em class="conn"></em></span>
          </div>
          <div class="notif" role="status"><b></b><span>New message</span></div>
          <div class="screens">
            <section class="screen screen--list">
              <h3>Chats</h3>
              <button type="button" class="chat-row">
                <span class="avatar">${peerOwner[0]}</span>
                <span class="chat-row__body"><b>${peerOwner}</b><span class="chat-row__preview"></span></span>
                <span class="badge" hidden></span>
              </button>
            </section>
            <section class="screen screen--chat">
              <header class="chat-head">
                <button type="button" class="back" title="Back to the chat list" aria-label="Back to the chat list">&#8249;</button>
                <span class="avatar">${peerOwner[0]}</span><b>${peerOwner}</b>
              </header>
              <div class="bubbles"></div>
              <form class="composer">
                <input type="text" placeholder="Message ${peerOwner}" autocomplete="off" aria-label="Message ${peerOwner}">
                <button type="submit" aria-label="Send"><svg viewBox="0 0 20 20" aria-hidden="true"><path d="M3 10l14-7-4 14-3-6z"/></svg></button>
              </form>
            </section>
          </div>
        </div>
      </div>
      <div class="phone-card__controls">
        <label class="switch"><input type="checkbox" class="wifi-toggle" checked><span class="switch__track"></span>Wi-Fi</label>
        <button type="button" class="ghost chat-toggle">Open chat</button>
      </div>
    </div>`;

  const $ = (selector) => root.querySelector(selector);
  const phoneEl = $('.phone');
  const bubbles = $('.bubbles');
  const input = $('.composer input');
  const known = new Map(); // message id -> bubble element
  let signature = '';

  $('.composer').addEventListener('submit', (event) => {
    event.preventDefault();
    const text = input.value.trim();
    if (!text) return;
    input.value = '';
    onSend(text);
  });
  $('.wifi-toggle').addEventListener('change', (event) => onWifi(event.target.checked));
  $('.chat-toggle').addEventListener('click', () => onChat());
  $('.chat-row').addEventListener('click', () => onChat(true));
  $('.back').addEventListener('click', () => onChat(false));

  function bubbleFor(message, mine) {
    const el = document.createElement('div');
    el.className = `bubble ${mine ? 'bubble--out' : 'bubble--in'}`;
    el.dataset.id = message.id;
    el.innerHTML = `<span class="bubble__text"></span><span class="bubble__meta"><span class="seq"></span><time></time>${mine ? `<span class="ticks">${TICKS_SVG}</span>` : ''}</span>`;
    el.querySelector('.bubble__text').textContent = message.text;
    return el;
  }

  function render(phone, now, wifiOn) {
    const messages = phone.messages();
    const sig = [wifiOn, phone.connection, phone.chatOpen, phone.notifications, phone.unread(), ...messages.map((m) => `${m.id}:${m.state}:${m.seq}`)].join('|');
    $('.statusbar__time').textContent = formatTime(now);
    if (sig === signature) return;
    signature = sig;

    phoneEl.classList.toggle('is-chat', phone.chatOpen);
    phoneEl.classList.toggle('is-offline', phone.connection === 'offline');
    phoneEl.dataset.connection = phone.connection;
    $('.conn').textContent = CONNECTION_TEXT[phone.connection];
    $('.wifi-toggle').checked = wifiOn;
    $('.chat-toggle').textContent = phone.chatOpen ? 'Close chat' : 'Open chat';

    const notif = $('.notif');
    notif.classList.toggle('is-shown', phone.notifications > 0);
    notif.querySelector('b').textContent = peerOwner;

    const badge = $('.badge');
    badge.hidden = phone.unread() === 0;
    badge.textContent = phone.unread();
    const last = messages[messages.length - 1];
    $('.chat-row__preview').textContent = last ? `${last.from === phone.name ? 'You: ' : ''}${last.text}` : 'No messages yet';

    // keyed update: existing bubbles keep their element (and their animation), new ones pop in
    const wanted = new Set(messages.map((m) => m.id));
    for (const [id, el] of known) {
      if (!wanted.has(id)) {
        el.remove();
        known.delete(id);
      }
    }
    messages.forEach((message, index) => {
      const mine = message.from === phone.name;
      let el = known.get(message.id);
      if (!el) {
        el = bubbleFor(message, mine);
        known.set(message.id, el);
      }
      if (bubbles.children[index] !== el) bubbles.insertBefore(el, bubbles.children[index] ?? null);
      el.querySelector('time').textContent = formatTime(message.createdAt);
      el.querySelector('.seq').textContent = message.seq === null ? '' : `#${message.seq}`;
      if (mine) {
        const tick = tickFor(message.state);
        const ticks = el.querySelector('.ticks');
        ticks.className = `ticks ticks--${tick}`;
        ticks.title = TICK_LABELS[tick];
      }
    });
    bubbles.scrollTop = bubbles.scrollHeight;
  }

  return { render, element: phoneEl, card: root.firstElementChild, reset() { known.clear(); bubbles.innerHTML = ''; signature = ''; } };
}
