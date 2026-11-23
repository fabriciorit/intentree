// One-click stories. Each scenario is a script of timed steps on the world's virtual clock, so
// it plays at whatever speed the page runs time, and tests can run it in an instant.

export const SCENARIOS = [
  {
    id: 'offline',
    label: 'Send while B is offline',
    steps: [
      [0, (w) => w.control.openChat('A')],
      [300, (w) => w.control.goOffline('B')],
      [1200, (w) => w.control.sendMessage('A', 'Are you there?')],
      [9000, (w) => w.control.goOnline('B')],
      [13500, (w) => w.control.openChat('B')],
    ],
  },
  {
    id: 'both',
    label: 'Both phones talk at once',
    steps: [
      [0, (w) => w.control.setLatency(800)],
      [100, (w) => w.control.openChat('A')],
      [100, (w) => w.control.openChat('B')],
      [800, (w) => w.control.sendMessage('A', 'Hi Bob!')],
      [800, (w) => w.control.sendMessage('B', 'Hi Alice!')],
      [2200, (w) => w.control.sendMessage('A', 'Dinner at 8?')],
      [2300, (w) => w.control.sendMessage('B', 'Sure, where?')],
    ],
  },
  {
    id: 'flaky',
    label: 'Flaky network',
    steps: [
      [0, (w) => w.control.openChat('A')],
      [0, (w) => w.control.flakyNetwork()],
      [800, (w) => w.control.sendMessage('A', 'Can you hear me?')],
      [2500, (w) => w.control.sendMessage('A', 'The signal is bad')],
      [4500, (w) => w.control.openChat('B')],
      [5000, (w) => w.control.sendMessage('A', 'Hello??')],
    ],
  },
];

export function runScenario(world, scenario) {
  for (const [at, step] of scenario.steps) world.clock.after(at, () => step(world));
}
