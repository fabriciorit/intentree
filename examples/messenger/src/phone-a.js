// Alice's phone. The chat client is shared; this module only says who she is and who she writes to.

import { Phone } from './phone.js';

export const PHONE_A = { name: 'phone A', tag: 'A', peer: 'phone B', owner: 'Alice' };

export function createPhoneA(deps) {
  return new Phone({ ...deps, name: PHONE_A.name, tag: PHONE_A.tag, peer: PHONE_A.peer });
}
