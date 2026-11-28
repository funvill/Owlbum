// A small W3C WebDriver client: just what the smoke test needs, with no
// packages to install. It talks to tauri-driver (docs/08-desktop-and-release.md,
// "The layers", row 5).

export const KEY = {
  control: "",
  shift: "",
  escape: "",
  f1: "",
  f2: "",
  f11: "",
};

export class Session {
  constructor(base, id) {
    this.base = base;
    this.id = id;
  }

  /** Starts the program; waits up to 10 s for the driver to listen. */
  static async start(base, capabilities) {
    await until("the driver", () => fetch(`${base}/status`).then((r) => r.ok), 10000);
    const reply = await call(base, "POST", "/session", { capabilities: { alwaysMatch: capabilities } });
    return new Session(base, reply.sessionId);
  }

  cmd(method, path, body) {
    return call(this.base, method, `/session/${this.id}${path}`, body);
  }

  /** Runs `body` (a function's text) in the page and answers its result. */
  run(fn, ...args) {
    return this.cmd("POST", "/execute/sync", { script: `return (${fn}).apply(null, arguments)`, args });
  }

  /**
   * Clicks, as a person would (focus moves too), the element `fn` answers
   * in the page; throws when it answers none.
   */
  async click(fn, ...args) {
    const ref = await this.run(fn, ...args);
    const id = ref && Object.values(ref)[0];
    if (!id) throw new Error(`nothing to click: ${fn}`);
    await this.cmd("POST", `/element/${id}/click`, {});
  }

  /** Presses keys: each entry is one key, or an array held together (a chord). */
  async press(...keys) {
    for (const k of keys) {
      const chord = Array.isArray(k) ? k : [k];
      const actions = [
        ...chord.map((value) => ({ type: "keyDown", value })),
        ...[...chord].reverse().map((value) => ({ type: "keyUp", value })),
      ];
      await this.cmd("POST", "/actions", { actions: [{ type: "key", id: "keyboard", actions }] });
      await this.cmd("DELETE", "/actions");
    }
  }

  /** The browser's console log, when the driver keeps one; null otherwise. */
  async browserLog() {
    try {
      return await this.cmd("POST", "/se/log", { type: "browser" });
    } catch {
      return null;
    }
  }

  async end() {
    try {
      await call(this.base, "DELETE", `/session/${this.id}`);
    } catch {
      // The program may have quit already.
    }
  }
}

async function call(base, method, path, body) {
  const res = await fetch(base + path, {
    method,
    headers: body === undefined ? undefined : { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  const json = text ? JSON.parse(text) : {};
  if (!res.ok) {
    const v = json.value ?? {};
    throw new Error(`${method} ${path}: ${res.status} ${v.error ?? ""} ${v.message ?? text}`);
  }
  // A script that answers null answers { value: null }.
  return "value" in json ? json.value : json;
}

/** Polls `check` until it answers something truthy, or throws after `ms`. */
export async function until(what, check, ms = 15000) {
  const end = Date.now() + ms;
  let last;
  for (;;) {
    try {
      last = await check();
      if (last) return last;
    } catch (e) {
      last = e;
    }
    if (Date.now() > end) throw new Error(`timed out waiting for ${what} (last: ${last instanceof Error ? last.message : JSON.stringify(last)})`);
    await new Promise((r) => setTimeout(r, 200));
  }
}
