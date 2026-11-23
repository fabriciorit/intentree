// Bob's phone. The chat client is shared; this module only says who he is and who he writes to.

import { Phone } from './phone.js';

export const PHONE_B = { name: 'phone B', tag: 'B', peer: 'phone A', owner: 'Bob' };

export function createPhoneB(deps) {
  return new Phone({ ...deps, name: PHONE_B.name, tag: PHONE_B.tag, peer: PHONE_B.peer });
}
