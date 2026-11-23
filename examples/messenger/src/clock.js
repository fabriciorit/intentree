// Virtual clock shared by every actor. Time only moves when someone calls advance(), so a
// page can pause, slow down or speed up the whole system, and tests run in microseconds.

export class Clock {
  #now = 0;
  #seq = 0;
  #timers = [];

  now() {
    return this.#now;
  }

  // Runs callback after delayMs of virtual time. Returns an id for cancel().
  after(delayMs, callback) {
    const timer = { id: ++this.#seq, due: this.#now + Math.max(0, delayMs), callback };
    // the timer queue is ordered by due time; equal due times keep scheduling order
    let at = this.#timers.length;
    while (at > 0 && this.#timers[at - 1].due > timer.due) at -= 1;
    this.#timers.splice(at, 0, timer);
    return timer.id;
  }

  cancel(id) {
    const at = this.#timers.findIndex((t) => t.id === id);
    if (at >= 0) this.#timers.splice(at, 1);
  }

  // Moves time forward, firing every timer that falls due on the way, in order.
  advance(ms) {
    const target = this.#now + ms;
    while (this.#timers.length > 0 && this.#timers[0].due <= target) {
      const timer = this.#timers.shift();
      this.#now = timer.due;
      timer.callback();
    }
    this.#now = target;
  }

  pending() {
    return this.#timers.length;
  }
}
