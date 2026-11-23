# Parking garage

A paid parking garage with barriers, simulated in the browser: two entries, one exit, a payment
kiosk and a controller that owns the count of free spaces, the ticket book and the clock.

It is both a small interactive simulator and a real project that intentree describes: the
architecture is written in `system.intent` and the views are generated from this very code.

- Each actor (controller, entry gate 1, entry gate 2, exit gate, payment kiosk) is its own ES
  module in `domain/`, with no DOM access, and they talk only through the message bus
  (`domain/bus.js`).
- Two state machines are declared and drawn live: Ticket (`issued`, `paid`, `expired`, `exited`)
  and Barrier (`closed`, `opening`, `open`, `closing`, `blocked`).
- `ui/` only draws the domain; `demo.json` is the manifest for the page shell (tour, views,
  example questions); `window.demo.actions` exposes `arriveAtEntry1`, `arriveAtEntry2`,
  `rushHour`, `payTicket`, `tryLeaveUnpaid`, `leave`, `waitPastGrace` and
  `obstacleUnderBarrier`.

## Open it

No dependencies and no build step. From the repository root:

```
python3 -m http.server
```

and open <http://localhost:8000/examples/parking/>.

## Tests

From this directory (needs only Node):

```
node --test
```

## Check the description

```
intentree check
```
