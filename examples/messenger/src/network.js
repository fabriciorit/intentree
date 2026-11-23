// The wire. Actors never call each other: they attach an address and send packets, and the
// network delivers them after a latency, unless the packet is lost or a Wi-Fi link is down.

// Small deterministic random generator, so a "flaky" run repeats exactly.
export function seededRandom(seed) {
  let state = seed >>> 0;
  return function next() {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

// Phones reach the server over Wi-Fi (adjustable, can be lost or cut). The push gateway
// reaches phones over the cellular channel, which keeps working when Wi-Fi is off.
const BACKBONE_MS = 60;
const CELLULAR_MS = 500;

export class Network {
  #clock;
  #observe;
  #random;
  #latencyMs;
  #jitterMs;
  #lossRate;
  #seq = 0;
  #endpoints = new Map();
  #offline = new Set();
  #inFlight = new Map();

  constructor(clock, { latencyMs = 300, jitterMs = 0, lossRate = 0, random = seededRandom(7), observe = () => {} } = {}) {
    this.#clock = clock;
    this.#latencyMs = latencyMs;
    this.#jitterMs = jitterMs;
    this.#lossRate = lossRate;
    this.#random = random;
    this.#observe = observe;
  }

  attach(address, handler) {
    this.#endpoints.set(address, { handler, watchers: [] });
  }

  watchLink(address, callback) {
    this.#endpoints.get(address).watchers.push(callback);
  }

  setLatency(ms) {
    this.#latencyMs = Math.max(0, ms);
  }

  setJitter(ms) {
    this.#jitterMs = Math.max(0, ms);
  }

  setLoss(rate) {
    this.#lossRate = Math.min(1, Math.max(0, rate));
  }

  settings() {
    return { latencyMs: this.#latencyMs, jitterMs: this.#jitterMs, lossRate: this.#lossRate };
  }

  isOffline(address) {
    return this.#offline.has(address);
  }

  // The per-phone offline switch: cuts the Wi-Fi link and tells the phone about it.
  setOffline(address, offline) {
    if (this.#offline.has(address) === offline) return;
    if (offline) this.#offline.add(address);
    else this.#offline.delete(address);
    this.#observe({ entity: 'network', kind: 'link', detail: { address, up: !offline } });
    for (const watcher of this.#endpoints.get(address).watchers) watcher(!offline);
  }

  inFlightPackets() {
    return [...this.#inFlight.values()];
  }

  #travelMs(channel) {
    if (channel === 'backbone') return BACKBONE_MS;
    if (channel === 'cellular') return CELLULAR_MS;
    const jitter = (this.#random() * 2 - 1) * this.#jitterMs;
    return Math.max(1, Math.round(this.#latencyMs + jitter));
  }

  send(from, to, type, body, channel = 'wifi') {
    const now = this.#clock.now();
    const travelMs = this.#travelMs(channel);
    const packet = {
      id: ++this.#seq,
      from,
      to,
      type,
      channel,
      body: JSON.parse(JSON.stringify(body ?? {})),
      sentAt: now,
      arriveAt: now + travelMs,
      fate: 'delivered',
    };
    let dropAt = 1;
    if (channel === 'wifi' && this.isOffline(from)) {
      packet.fate = 'offline';
      dropAt = 0.05;
    } else if (channel === 'wifi' && this.#random() < this.#lossRate) {
      packet.fate = 'lost';
      dropAt = 0.5;
    }
    this.#inFlight.set(packet.id, packet);
    this.#observe({ entity: 'network', kind: 'send', detail: { packet } });
    this.#clock.after(Math.round(travelMs * dropAt), () => this.#land(packet, dropAt));
    return packet;
  }

  #land(packet, dropAt) {
    this.#inFlight.delete(packet.id);
    let reason = null;
    if (packet.fate !== 'delivered') reason = packet.fate;
    else if (packet.channel === 'wifi' && this.isOffline(packet.to)) reason = 'offline';
    if (reason) {
      packet.fate = reason;
      this.#observe({ entity: 'network', kind: 'drop', detail: { packet, reason, at: reason === 'offline' && dropAt === 1 ? 1 : dropAt } });
      return;
    }
    this.#observe({ entity: 'network', kind: 'receive', detail: { packet } });
    this.#endpoints.get(packet.to).handler(packet);
  }
}
