// The geometry of the garage, in drawing units (no DOM here): lanes, spaces and the routes
// cars follow. The scene draws it and the traffic walks along it.

export const VIEW = Object.freeze({ width: 960, height: 580 });
export const WALL_Y = 430;
export const BARRIER_Y = 445;
export const ZONE = Object.freeze({ top: 422, bottom: 468, half: 32 }); // under the barrier arm
export const LANE_X = [130, 250];
export const EXIT_X = 900;
export const HEAD_Y = 492; // first car waiting at an entry
export const EXIT_HEAD_Y = 398; // first car waiting at the exit
export const QUEUE_GAP = 46;
export const OFFSCREEN_Y = 640;
export const AISLE_LOW = 360;
export const AISLE_HIGH = 160;
export const CONNECTOR_LEFT = 320;
export const CONNECTOR_RIGHT = 760;
export const KIOSK_STOP = Object.freeze({ x: 815, y: AISLE_LOW });
export const KIOSK_BOX = Object.freeze({ x: 764, y: 388, width: 100, height: 38 });
export const COLUMNS = [410, 500, 590, 680];
export const ROWS = [
  { name: 'C', y: 300 },
  { name: 'B', y: 230 },
  { name: 'A', y: 90 },
];

// Twelve spaces, nearest first: C1..C4, B1..B4, A1..A4.
export const SLOTS = ROWS.flatMap((row) =>
  COLUMNS.map((x, i) => ({ id: `${row.name}${i + 1}`, row: row.name, x, y: row.y })),
);

const pt = (x, y) => ({ x, y });

export function entryRoute(lane, slot) {
  const x = LANE_X[lane];
  const route = [pt(x, HEAD_Y), pt(x, AISLE_LOW)];
  if (slot.row === 'C') route.push(pt(slot.x, AISLE_LOW));
  else route.push(pt(CONNECTOR_LEFT, AISLE_LOW), pt(CONNECTOR_LEFT, AISLE_HIGH), pt(slot.x, AISLE_HIGH));
  route.push(pt(slot.x, slot.y));
  return route;
}

// From the space to the low aisle, east of every space.
export function slotToAisle(slot) {
  if (slot.row === 'C') return [pt(slot.x, slot.y), pt(slot.x, AISLE_LOW), pt(CONNECTOR_RIGHT, AISLE_LOW)];
  return [pt(slot.x, slot.y), pt(slot.x, AISLE_HIGH), pt(CONNECTOR_RIGHT, AISLE_HIGH), pt(CONNECTOR_RIGHT, AISLE_LOW)];
}

export const slotToKiosk = (slot) => [...slotToAisle(slot), pt(KIOSK_STOP.x, KIOSK_STOP.y)];
export const kioskToExit = () => [pt(KIOSK_STOP.x, KIOSK_STOP.y), pt(EXIT_X, AISLE_LOW)];
export const slotToExit = (slot) => [...slotToAisle(slot), pt(EXIT_X, AISLE_LOW)];
export const exitToSlot = (slot) => [pt(EXIT_X, EXIT_HEAD_Y), ...slotToAisle(slot).reverse()];
export const passExit = () => [pt(EXIT_X, EXIT_HEAD_Y), pt(EXIT_X, OFFSCREEN_Y)];
export const reverseRoute = (route) => [...route].reverse();
