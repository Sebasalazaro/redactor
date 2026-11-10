// A fake backend for working on the dashboard in a plain browser
// (`npm run dev`, then open /dashboard.html). Loaded only in dev builds and
// only outside Tauri. Every value here is fictional.

type Args = Record<string, unknown> | undefined;

const config = (over: Record<string, unknown> = {}) => ({
  name: null,
  client: [],
  users: [],
  hosts: [],
  terms: [],
  allow_domains: [],
  sensitive_headers: [],
  sensitive_keys: [],
  masking: null,
  ...over,
});

const db = {
  settings: {
    hotkey: "CommandOrControl+Alt+R",
    active_engagement: "globex-q3",
    review: true,
    keep_last: true,
    forget_last_after_minutes: 15,
    clear_clipboard_on_cancel: true,
    remember: true,
    unload_windows: true,
    launch_at_login: false,
    close_notice_shown: true,
  },
  global: config({
    client: ["Globex Bank"],
    terms: [{ value: "MacBook-de-Pentester", replacement: "[DEVICE]" }],
    users: ["qa.tester01"],
  }),
  engagements: {
    "globex-q3": config({
      name: "Globex Q3 web app",
      terms: [
        { value: "Wavepay", replacement: "[APP]" },
        { value: "sbus", replacement: "[SYSTEM]" },
      ],
      hosts: ["srv-db01"],
    }),
    "initech-api": config({ name: "Initech API", terms: [{ value: "TPS", replacement: null }] }),
  } as Record<string, ReturnType<typeof config>>,
};

const LAST = `GET /dataingestion/data-sources/88*** HTTP/1.1
Host: [SYSTEM].api.[CLIENT].com
Cookie: session=a7f3e9…[len=32]
X-Forwarded-For: 203.0.*.*`;

const handlers: Record<string, (a: Args) => unknown> = {
  get_status: () => ({
    config_dir: "C:\\Users\\tester\\.config\\redactor",
    model_dir: "C:\\Users\\tester\\.config\\redactor\\models",
    ai_installed: false,
    error: null,
    hotkey_active: true,
    hotkey_last_press_secs: 42,
    log_file: "C:\\Users\\tester\\.config\\redactor\\redactor.log",
    version: "0.3.0-dev",
  }),
  get_settings: () => structuredClone(db.settings),
  save_settings: (a) => void (db.settings = (a as { settings: typeof db.settings }).settings),
  get_global: () => structuredClone(db.global),
  save_global: (a) => void (db.global = (a as { config: typeof db.global }).config),
  list_engagements: () =>
    Object.entries(db.engagements).map(([id, c]) => ({ id, name: c.name ?? id, config: structuredClone(c) })),
  get_engagement: (a) => structuredClone(db.engagements[(a as { name: string }).name]),
  save_engagement: (a) => {
    const { name, config: c } = a as { name: string; config: ReturnType<typeof config> };
    db.engagements[name] = c;
  },
  create_engagement: (a) => {
    const name = (a as { name: string }).name;
    const id = name.toLowerCase().replace(/[^a-z0-9]+/g, "-");
    db.engagements[id] = config({ name });
    return id;
  },
  delete_engagement: (a) => void delete db.engagements[(a as { name: string }).name],
  get_overview: () => ({
    last: {
      output: LAST,
      format: "http",
      categories: [
        ["client", 1],
        ["custom", 1],
        ["secret", 1],
        ["ip", 1],
        ["id", 1],
      ],
      engagement: "globex-q3",
      age_secs: 95,
    },
    forgotten: null,
    memory: {
      app_bytes: 24e6,
      webview_bytes: 61e6,
      webview_processes: 2,
      ai_bytes: 0,
      system_total: 16e9,
      system_used: 9e9,
    },
    redactions: 7,
    uptime_secs: 3600,
    ai_installed: false,
    ai_loaded: false,
  }),
  preview: (a) => ({
    format: "text",
    ai_installed: false,
    engagement: null,
    segments: [{ kind: "plain", text: (a as { text: string }).text }],
  }),
  vault_info: () => ({ profile: "globex-q3", entries: 12 }),
  restore_text: (a) => ({ parts: [{ kind: "plain", text: (a as { text: string }).text }] }),
  get_compare: () => ({ high_file: null, low_file: null, report: null }),
  free_memory: () => ({ released_bytes: 3e6, memory: {} }),
};

let callbacks = 0;
(window as unknown as { __TAURI_EVENT_PLUGIN_INTERNALS__: unknown }).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
  unregisterListener: () => {},
};
(window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
  transformCallback: () => ++callbacks,
  unregisterCallback: () => {},
  invoke: async (cmd: string, args: Args) => {
    if (cmd.startsWith("plugin:event|")) return cmd.endsWith("listen") ? ++callbacks : null;
    const handler = handlers[cmd];
    if (!handler) {
      console.info(`devmock: ${cmd}`, args);
      return null;
    }
    return handler(args);
  },
};

export {};
