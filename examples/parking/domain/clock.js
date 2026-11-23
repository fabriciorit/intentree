// Simulated time, in seconds. Nothing in the garage reads the wall clock.

export function createClock(start = 8 * 3600) {
  let now = start;
  return {
    now: () => now,
    advance(seconds) {
      now += seconds;
      return now;
    },
  };
}

export function formatClock(seconds) {
  const s = Math.floor(seconds) % 86400;
  const pad = (n) => String(n).padStart(2, '0');
  return `${pad(Math.floor(s / 3600))}:${pad(Math.floor((s % 3600) / 60))}:${pad(s % 60)}`;
}
