// Puts the whole system together: one clock, one network, the server, the push gateway and two
// phones. Also the control panel the page and the scenarios use: it can cut a phone's Wi-Fi,
// change the latency, and make a phone write or open the chat.

import { Clock } from './clock.js';
import { Network, seededRandom } from './network.js';
import { Server } from './server.js';
import { PushGateway } from './push-gateway.js';
import { createPhoneA } from './phone-a.js';
import { createPhoneB } from './phone-b.js';

const SAMPLE_TEXTS = ['Hi!', 'Are you there?', 'Look at this', 'Running late', 'Dinner at 8?', 'On my way', 'Call me when you can', 'Haha, really?'];

export function createWorld({ seed = 7, latencyMs = 300, warmUpMs = 1500 } = {}) {
  const clock = new Clock();
  const log = [];
  const listeners = [];
  const observe = (event) => {
    const stamped = { ...event, at: clock.now() };
    log.push(stamped);
    for (const listener of listeners) listener(stamped);
  };

  const network = new Network(clock, { latencyMs, random: seededRandom(seed), observe });
  const server = new Server({ clock, network, observe });
  const gateway = new PushGateway({ network, observe });
  const phones = { A: createPhoneA({ clock, network, observe }), B: createPhoneB({ clock, network, observe }) };
  phones.A.openChat(); // Alice starts inside the chat; Bob sees his chat list
  phones.A.start();
  phones.B.start();
  clock.advance(warmUpMs); // both phones connect before anything else happens

  let samples = 0;
  const control = {
    sendMessage(from = 'A', text = SAMPLE_TEXTS[samples++ % SAMPLE_TEXTS.length]) {
      return phones[from].send(text);
    },
    goOffline(who) {
      network.setOffline(phones[who].name, true);
    },
    goOnline(who) {
      network.setOffline(phones[who].name, false);
    },
    openChat(who) {
      phones[who].openChat();
    },
    closeChat(who) {
      phones[who].closeChat();
    },
    setLatency(ms) {
      network.setLatency(ms);
    },
    setLoss(rate) {
      network.setLoss(rate);
    },
    // A bad network: slow, jittery and dropping about a third of the packets.
    flakyNetwork() {
      network.setLatency(600);
      network.setJitter(400);
      network.setLoss(0.35);
    },
    steadyNetwork() {
      network.setLatency(300);
      network.setJitter(0);
      network.setLoss(0);
    },
  };

  return {
    clock,
    network,
    server,
    gateway,
    phones,
    control,
    log,
    advance: (ms) => clock.advance(ms),
    subscribe(listener) {
      listeners.push(listener);
    },
  };
}
