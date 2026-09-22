// Debounced saving for dashboard forms. Each document is identified by a
// key; `mark` records what is on disk so loading never triggers a save.

/** Window event asking form fields to commit half-typed values. */
export const COMMIT_DRAFTS = "redactor:commit-drafts";

export type SaveState = "idle" | "pending" | "saving" | "saved" | "error";

export class Autosave {
  state = $state<SaveState>("idle");
  error = $state("");
  #saved = new Map<string, string>();
  #timers = new Map<string, ReturnType<typeof setTimeout>>();
  #pending = new Map<string, () => Promise<void>>();
  #delay: number;

  constructor(delay = 500) {
    this.#delay = delay;
  }

  mark(key: string, value: unknown) {
    this.#saved.set(key, JSON.stringify(value));
  }

  /** Schedules `save` if `value` differs from what was last saved. */
  track(key: string, value: unknown, save: () => Promise<unknown>) {
    const json = JSON.stringify(value);
    if (!this.#saved.has(key) || this.#saved.get(key) === json) return;
    this.state = "pending";
    clearTimeout(this.#timers.get(key));
    const run = () => this.#run(key, json, save);
    this.#pending.set(key, run);
    this.#timers.set(key, setTimeout(run, this.#delay));
  }

  /** Runs every scheduled save now (e.g. before the window closes). */
  async flush() {
    const runs = [...this.#pending.values()];
    for (const timer of this.#timers.values()) clearTimeout(timer);
    this.#timers.clear();
    await Promise.all(runs.map((run) => run()));
  }

  /** Saves immediately, cancelling any scheduled save for `key`. */
  now(key: string, value: unknown, save: () => Promise<unknown>) {
    clearTimeout(this.#timers.get(key));
    this.#pending.delete(key);
    return this.#run(key, JSON.stringify(value), save);
  }

  async #run(key: string, json: string, save: () => Promise<unknown>) {
    this.#pending.delete(key);
    this.state = "saving";
    try {
      await save();
      this.#saved.set(key, json);
      this.state = "saved";
    } catch (e) {
      this.state = "error";
      this.error = String(e);
    }
  }
}
