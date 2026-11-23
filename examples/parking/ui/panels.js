// The side panels: ticket book, live state machines and the event log.

import { TICKET_STATES, TICKET_TRANSITIONS } from '../domain/ticket.js';
import { BARRIER_STATES, BARRIER_TRANSITIONS } from '../domain/barrier.js';
import { MSG } from '../domain/protocol.js';
import { formatClock } from '../domain/clock.js';
import { createMachineDiagram } from './machines.js';

const GATE_SHORT = { 'entry gate 1': 'E1', 'entry gate 2': 'E2', 'exit gate': 'X' };

export function createPanels({ garage, ticketList, ticketSummary, machines, logList }) {
  // --- ticket book
  let lastTickets = '';
  function renderTickets() {
    const book = garage.controller.ticketBook();
    const key = JSON.stringify(book.map((t) => [t.id, t.state, t.paidTotal]));
    if (key === lastTickets) return;
    lastTickets = key;
    const inside = book.filter((t) => t.state !== 'exited').length;
    ticketSummary.textContent = `${inside} in the garage, ${book.length} issued so far`;
    ticketList.replaceChildren();
    const rows = [...book].reverse().slice(0, 40);
    if (rows.length === 0) {
      const empty = document.createElement('li');
      empty.className = 'empty';
      empty.textContent = 'No tickets yet. Send a car in.';
      ticketList.appendChild(empty);
    }
    for (const t of rows) {
      const li = document.createElement('li');
      li.className = 'ticket';
      const id = document.createElement('b');
      id.textContent = t.id;
      const gate = document.createElement('span');
      gate.className = 'muted';
      gate.textContent = GATE_SHORT[t.gate];
      const chip = document.createElement('span');
      chip.className = `chip ${t.state}`;
      chip.textContent = t.state;
      const fee = document.createElement('span');
      fee.className = 'muted fee';
      fee.textContent = t.paidTotal > 0 ? t.paidTotal.toFixed(2) : '';
      li.append(id, gate, chip, fee);
      ticketList.appendChild(li);
    }
  }

  // --- state machines
  const ticketDiagram = createMachineDiagram(machines.ticket, {
    name: 'Ticket', states: TICKET_STATES, transitions: TICKET_TRANSITIONS, width: 380, height: 150,
    positions: { issued: { x: 60, y: 44 }, paid: { x: 190, y: 44 }, exited: { x: 320, y: 44 }, expired: { x: 190, y: 120 } },
  });
  const barrierDiagram = createMachineDiagram(machines.barrier, {
    name: 'Barrier', states: BARRIER_STATES, transitions: BARRIER_TRANSITIONS, width: 440, height: 150,
    positions: { opening: { x: 70, y: 36 }, open: { x: 220, y: 36 }, closing: { x: 220, y: 120 }, closed: { x: 70, y: 120 }, blocked: { x: 370, y: 120 } },
  });
  function renderMachines() {
    const tickets = {};
    for (const t of garage.controller.ticketBook()) (tickets[t.state] ||= []).push(t.id);
    ticketDiagram.update(Object.fromEntries(Object.entries(tickets).map(([state, ids]) => [state, [String(ids.length)]])));
    const barriers = {};
    for (const gate of [garage.entry1, garage.entry2, garage.exit]) (barriers[gate.barrier.state] ||= []).push(GATE_SHORT[gate.name]);
    barrierDiagram.update(barriers);
  }

  // --- event log
  function describe(e) {
    if (e.kind === 'send') {
      if (e.type === MSG.FREE_COUNT) return null;
      return { cls: 'msg', text: `${e.entity} → ${e.to}: ${e.type}` };
    }
    if (e.kind === 'dropped') return { cls: 'warn', text: `${e.type} to ${e.entity} is lost (offline)` };
    if (e.kind === 'timeout') return { cls: 'warn', text: `${e.entity} gave up waiting for the controller` };
    if (e.kind === 'state' && e.machine === 'Ticket' && e.from === e.to) return { cls: 'warn', text: `ticket ${e.ticket}: used again, refused` };
    if (e.kind === 'state' && e.machine === 'Ticket') return { cls: 'state', text: `ticket ${e.ticket}: ${e.from || 'new'} → ${e.to}` };
    if (e.kind === 'state' && e.machine === 'Barrier') return { cls: e.to === 'blocked' ? 'warn' : 'state', text: `${e.entity}: barrier ${e.from} → ${e.to}` };
    if (e.kind === 'car') return { cls: 'car', text: e.text };
    return null;
  }
  garage.onEvent((e) => {
    const line = describe(e);
    if (!line) return;
    const li = document.createElement('li');
    li.className = line.cls;
    const time = document.createElement('time');
    time.textContent = formatClock(e.at);
    const span = document.createElement('span');
    span.textContent = line.text;
    li.append(time, span);
    logList.prepend(li);
    while (logList.children.length > 120) logList.lastChild.remove();
  });

  return { render() { renderTickets(); renderMachines(); } };
}
