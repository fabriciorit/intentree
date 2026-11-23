// The in-page message bus: the only way the actors reach each other, like a network.
// Messages are queued, delivered in order (time, sequence), can be delayed (latency) and are
// lost when the destination is offline.

export function createBus({ clock }) {
  const endpoints = new Map();
  const offline = new Set();
  const listeners = [];
  const queue = [];
  let sequence = 0;
  let latency = 0;

  function onEvent(callback) {
    listeners.push(callback);
  }

  // Everything an actor does worth showing goes through here (send, receive, state, ...).
  function emit(entity, kind, detail = {}) {
    const event = { entity, kind, at: clock.now(), ...detail };
    for (const callback of listeners) callback(event);
  }

  function register(name, handler) {
    endpoints.set(name, handler);
  }

  function send(from, to, message) {
    emit(from, 'send', { to, type: message.type });
    queue.push({ from, to, message, due: clock.now() + latency, sequence: sequence++ });
  }

  function nextDue() {
    let best = -1;
    for (let i = 0; i < queue.length; i++) {
      const m = queue[i];
      if (m.due > clock.now()) continue;
      if (best < 0 || m.due < queue[best].due || (m.due === queue[best].due && m.sequence < queue[best].sequence)) best = i;
    }
    return best;
  }

  // Delivers every message that is due, including the replies they trigger.
  function deliver() {
    for (let index = nextDue(); index >= 0; index = nextDue()) {
      const [m] = queue.splice(index, 1);
      if (offline.has(m.to) || !endpoints.has(m.to)) {
        emit(m.to, 'dropped', { from: m.from, type: m.message.type });
        continue;
      }
      emit(m.to, 'receive', { from: m.from, type: m.message.type });
      endpoints.get(m.to)(m.message, m.from);
    }
  }

  return {
    onEvent,
    emit,
    register,
    send,
    deliver,
    setLatency: (seconds) => { latency = Math.max(0, seconds); },
    getLatency: () => latency,
    setOffline: (name, value) => { if (value) offline.add(name); else offline.delete(name); },
    isOffline: (name) => offline.has(name),
    pending: () => queue.length,
  };
}
