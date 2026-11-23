// The push gateway: an outside service that nudges a phone that cannot be reached over the
// internet. It only obeys the server, and it reaches the phone over the cellular channel.

export class PushGateway {
  #network;
  #observe;
  #address;
  #trusted;
  #sent = new Map();

  constructor({ network, observe = () => {}, address = 'push gateway', trusted = 'server' }) {
    this.#network = network;
    this.#observe = observe;
    this.#address = address;
    this.#trusted = trusted;
    network.attach(address, (packet) => this.handle(packet));
  }

  pushesTo(phone) {
    return this.#sent.get(phone) ?? 0;
  }

  total() {
    let sum = 0;
    for (const n of this.#sent.values()) sum += n;
    return sum;
  }

  handle(packet) {
    this.#observe({ entity: this.#address, kind: 'receive', detail: { from: packet.from, type: packet.type } });
    if (packet.type !== 'push request') return;
    if (packet.from !== this.#trusted) {
      this.#observe({ entity: this.#address, kind: 'reject', detail: { from: packet.from } });
      return;
    }
    const phone = packet.body.to;
    this.#sent.set(phone, this.pushesTo(phone) + 1);
    this.#observe({ entity: this.#address, kind: 'send', detail: { to: phone, type: 'push' } });
    this.#network.send(this.#address, phone, 'push', { preview: packet.body.preview }, 'cellular');
  }
}
