// The life of one chat message, as the ticks show it:
//   composing -> queued (clock) -> sent (one grey tick) -> delivered (two grey) -> read (two blue)
// Everything that needs to move a message goes through advanceTo(), which can only walk
// forward one state at a time. That is what makes "never read before delivered" true.

export const MESSAGE_STATES = ['composing', 'queued', 'sent', 'delivered', 'read'];

export const TICKS = {
  composing: 'draft',
  queued: 'clock',
  sent: 'one-grey',
  delivered: 'two-grey',
  read: 'two-blue',
};

export function stateRank(state) {
  const rank = MESSAGE_STATES.indexOf(state);
  if (rank < 0) throw new Error(`unknown message state: ${state}`);
  return rank;
}

// The steps from `current` to `target`, one state at a time. A target that is not ahead of
// the current state gives no steps: a late or duplicated notice can never move a tick back.
export function advanceTo(current, target) {
  const steps = [];
  for (let rank = stateRank(current); rank < stateRank(target); rank += 1) {
    steps.push({ from: MESSAGE_STATES[rank], to: MESSAGE_STATES[rank + 1] });
  }
  return steps;
}

export function tickFor(state) {
  return TICKS[state];
}
