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

type MockFile = Record<string, unknown>;

const BASE = "C:/Users/tester/Documents/Clientes";
let mockId = 0;

/** A fictional entry for the files dialog. */
function mock(source: string, segments: string[], values: number, extra: MockFile = {}): MockFile {
  mockId += 1;
  return {
    id: mockId,
    base: BASE,
    segments,
    source,
    status: "ready",
    message: null,
    format: "json",
    values,
    categories: values ? [["client", Math.ceil(values / 2)], ["secret", Math.floor(values / 2)]] : [],
    preview: `{\n  "cliente": "[CLIENT]",\n  "token": "a7f3e9…[len=32]"\n}`,
    has_text: false,
    folder: false,
    ...extra,
  };
}

const mockFolder = (): MockFile[] => [
  mock("Globex/2026/Globex_contrato.json", ["CLIENT.redacted", "2026", "CLIENT_contrato.json"], 6),
  mock("Globex/2026/Pagos noviembre.xlsx", ["CLIENT.redacted", "2026", "Pagos noviembre.xlsx"], 12, {
    format: "xlsx",
    has_text: true,
    categories: [
      ["pii", 7],
      ["client", 3],
      ["user", 2],
    ],
    preview: `## Pagos [CLIENT]\n\n| Fecha | Proveedor | NIT | Valor |\n| --- | --- | --- | --- |\n| 2026-11-03 | Ferr****** L* Un*** | 90*.12*.45*-* | 1250000 |`,
  }),
  mock("Globex/2026/terceros.csv", ["CLIENT.redacted", "2026", "terceros.csv"], 4, { format: "csv" }),
  mock("Globex/API.postman_collection.json", ["CLIENT.redacted", "API.postman_collection.json"], 14, {
    format: "postman",
  }),
  mock("Globex/notas.txt", ["CLIENT.redacted", "notas.txt"], 2, {
    format: "text",
    preview: "Reunión con [CLIENT] el martes",
  }),
  mock("Globex/contrato firmado.pdf", ["CLIENT.redacted", "contrato firmado.pdf"], 0, {
    status: "skipped",
    message: "not a text file",
    format: null,
    preview: null,
  }),
  mock("Globex/.git", ["CLIENT.redacted", ".git"], 0, {
    status: "skipped",
    message: "folder not read (tool files)",
    folder: true,
    format: null,
    preview: null,
  }),
];

let mockFiles: MockFile[] = [];

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
  get_files: () => mockFiles,
  pick_files: () => (mockFiles = [...mockFiles, mock("Globex Q3.postman_collection.json", ["CLIENT Q3.postman_collection.redacted.json"], 9, { base: "C:/Users/tester/Downloads", format: "postman" })]),
  pick_folder: () => (mockFiles = [...mockFiles, ...mockFolder()]),
  save_files: () =>
    (mockFiles = mockFiles.map((f) => (f.status === "ready" ? { ...f, status: "saved", preview: null } : f))),
  copy_file_text: () => null,
  remove_file: (a) => (mockFiles = mockFiles.filter((f) => f.id !== (a as { id: number }).id)),
  clear_files: () => (mockFiles = []),
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
