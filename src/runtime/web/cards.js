// Changed images stay here until IndexedDB acknowledges their transaction.
// An error retains the latest image for explicit retry and export backup.
import * as storage from './storage.js';
const key = (slot) => `card${slot + 1}`;
const check = (slot, image, limit) => {
  if (![0, 1].includes(slot) || !(image instanceof Uint8Array) || (image.length < 0x20000 || image.length > limit)) {
    throw new Error('Card slot is 0 or 1; image must be a 128 KiB card or bounded package-bound card.');
  }
};

export class Cards {
  constructor(app) {
    this.app = app;
    this.writing = null;
    this.operations = Promise.resolve();
    this.images = new Map();
    this.written = 0;
    this.error = null;
    this.notice = null;
    this.missing = new Set();
  }

  /** Serialize explicit card replacements with whole-session boot. A failed
   * operation rejects its caller but releases the queue for the next attempt. */
  exclusive(operation) {
    const result = this.operations.then(operation);
    this.operations = result.then(() => undefined, () => undefined);
    return result;
  }

  /** Hand both stored images to the runtime (none stored: a fresh formatted card). */
  async load() {
    this.missing.clear();
    for (const slot of [0, 1]) {
      const image = await storage.get('cards', key(slot));
      if (image) check(slot, image, this.app.card_file_limit());
      else this.missing.add(slot);
      this.app.set_card_image(slot, image ?? undefined);
    }
    this.updateNotice();
  }

  updateNotice() {
    const slots = [...this.missing].map((slot) => slot + 1);
    this.notice = slots.length ? `No stored card ${slots.join(' or ')}. Fresh cards are in use. Browser saves may have been cleared or evicted; import a backup if needed.` : null;
  }

  /** Start writing the images the card device changed; resolves when every write so far landed. */
  collect() {
    for (const slot of [0, 1]) {
      const image = this.app.take_card_write(slot);
      if (image) this.images.set(slot, image);
    }
  }
  flush({ retry = false } = {}) {
    this.collect();
    if (retry) this.error = null;
    if (this.writing) return this.writing;
    // A startup read error remains visible, but an empty queue has no writes
    // to settle. Failed pending images must still block session replacement.
    if (this.error && this.images.size) return Promise.reject(new Error(this.error));
    this.writing = (async () => {
      while (this.images.size) {
        const [slot, image] = this.images.entries().next().value;
        try {
          await storage.put('cards', key(slot), image);
          if (this.images.get(slot) === image) this.images.delete(slot);
          this.written++;
          this.missing.delete(slot);
          this.updateNotice();
        } catch (error) {
          this.error = String(error);
          throw error;
        }
      }
    })().finally(() => { this.writing = null; });
    return this.writing;
  }

  /** Persist a raw card and insert it into the live session between frames. */
  async store(slot, image) {
    check(slot, image, this.app.card_file_limit());
    await this.replace(slot, () => image);
  }

  /** Import a card or save file the player picked into card `slot`: the
   * runtime reads it (any supported card, or a fresh card holding a single
   * save; refused while the game has a file open on the card), then it is
   * stored and inserted as `store` does. Resolves to what was found
   * ("Card 1: DexDrive card, 3 saves"). */
  async importFile(slot, bytes, name) {
    let message;
    await this.replace(slot, () => {
      const found = this.app.read_card_file(slot, bytes, name);
      message = found.message;
      return found.image;
    });
    return message;
  }

  /** With play paused and pending writes landed, IndexedDB takes the card
   * `read` gives, then the runtime (between frames). */
  async replace(slot, read) {
    return this.exclusive(async () => {
      const running = JSON.parse(this.app.status()).running;
      this.app.set_running(false);
      try {
        await this.flush({ retry: true });
        const image = read();
        check(slot, image, this.app.card_file_limit());
        this.app.check_card_image(slot, image);
        await storage.put('cards', key(slot), image);
        try {
          this.app.set_card_image(slot, image);
        } catch (error) {
          // Refused only when frames ran meanwhile and the game opened a file
          // on the card: IndexedDB takes back the card the session keeps.
          const kept = this.app.card_image(slot);
          if (kept) await storage.put('cards', key(slot), kept);
          throw error;
        }
        this.missing.delete(slot);
        this.updateNotice();
      } finally {
        if (running) this.app.set_running(true);
      }
    });
  }

  /** The image IndexedDB holds for `slot`, or undefined. */
  stored(slot) {
    return storage.get('cards', key(slot));
  }

  status() {
    return { pending: this.images.size, written: this.written, error: this.error, notice: this.notice };
  }
}
