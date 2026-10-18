'use strict';
const { EventEmitter } = require('node:events');
const protocol = require('./protocol.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

class Controller extends EventEmitter {
  constructor(hid) {
    super(); this.hid = hid; this.device = null; this.sequence = 0;
    this.busy = false; this.status = { connected: false, message: 'Ready to connect', product: '' };
  }
  id() { this.sequence = this.sequence % 254 + 1; return this.sequence; }
  update(connected, message, product = '') {
    this.status = { connected, message, product }; this.emit('status', this.status);
  }
  async disconnect(message = 'Disconnected') {
    const device = this.device; this.device = null;
    if (device) { try { await device.close(); } catch {} }
    this.update(false, message);
  }
  async connect() {
    if (this.device) return this.status;
    if (this.busy) return this.status;
    this.busy = true;
    try {
      const devices = await this.hid.devicesAsync(protocol.VID, protocol.PID);
      // Never open the keyboard interface of this composite device.
      const target = devices.find(d => d.interface === 0 || /mi_00/i.test(d.path)) ||
        devices.find(d => d.usagePage >= 0xff00 && d.usagePage !== 1);
      if (!target) throw new Error('No compatible strip found. Check the USB connection.');
      const device = await this.hid.HIDAsync.open(target.path);
      this.device = device;
      device.on('error', error => { if (this.device === device) void this.disconnect(`USB disconnected: ${error.message}`); });
      // Start the read loop to drain command replies.
      device.on('data', () => {});
      await this.writePacket(protocol.command(this.id(), 0x82));
      await delay(50);
      await this.writePacket(protocol.command(this.id(), 0x93, [0]));
      await this.writePacket(protocol.command(this.id(), 0x87, [100]));
      this.update(true, 'Connected over USB', target.product || 'SyncLight-compatible strip');
    } catch (error) { await this.disconnect(`${error.message} Close SyncLight before connecting.`); }
    finally { this.busy = false; }
    return this.status;
  }
  async writePacket(packet) {
    const device = this.device;
    if (!device) throw new Error('Strip is not connected');
    for (const report of protocol.reports(packet)) {
      const written = await device.write([...report]);
      if (written !== report.length) throw new Error(`Incomplete USB write (${written}/${report.length})`);
    }
  }
  async send(packet) {
    if (!this.device || this.busy) return false;
    this.busy = true;
    try { await this.writePacket(packet); return true; }
    catch (error) { await this.disconnect(`USB write failed: ${error.message}`); return false; }
    finally { this.busy = false; }
  }
  async solid(rgb, count) { return this.send(protocol.solid(this.id(), rgb, count)); }
  async frame(colors) { return this.send(protocol.frame(this.id(), colors)); }
}
module.exports = { Controller };
