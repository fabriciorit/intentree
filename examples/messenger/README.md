# Messenger: ticks, packets and a server

A small, working WhatsApp-style messenger that runs in the browser: two phones, a server, a push
gateway and a network that carries packets with adjustable latency, optional loss and a Wi-Fi switch
per phone. Send a message and follow it from the clock icon (queued) to one grey tick (the server
stored it), two grey ticks (the other phone received it) and two blue ticks (the chat was opened).

It is also a real project described by intentree: `system.intent` states the architecture (layers,
processes, state machines, invariants with anchors into the code) and `demo.json` is the manifest a
page shell uses to show intentree's views next to the simulator.

## Open it

From the repository root:

```
python3 -m http.server
```

and open <http://localhost:8000/examples/messenger/>. There are no dependencies and no build step.

Try the one-click scenarios (send while B is offline, both phones talk at once, flaky network), cut a
phone's Wi-Fi, drag the latency and loss sliders, or slow time down to watch single packets.

## Layout

```
src/        domain, no DOM: clock, network, server, push gateway, phone client, message states
ui/         the page: phone and wire drawing, log, controls, window.demo
tests/      node:test unit tests for the domain
system.intent   the architecture, in controlled English
demo.json       manifest for the page shell (views, tour, sources)
```

The actors never call each other. They send packets through `src/network.js`, which delivers them
after the latency, unless the packet is lost or a phone's Wi-Fi is off. Time is a virtual clock
(`src/clock.js`), so the page can pause, speed up or slow down the whole system and tests run instantly.

## Test

```
cd examples/messenger
node --test
```

## Check the architecture description

```
intentree check
intentree view machine:message --format text
intentree where who decides the order of messages
```
