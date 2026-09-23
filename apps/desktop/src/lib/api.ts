// Typed wrappers around the Rust commands (see src-tauri/src/commands.rs).

import { invoke } from "@tauri-apps/api/core";

export type Category =
  | "private_key"
  | "jwt"
  | "credential"
  | "secret"
  | "card"
  | "email"
  | "uuid"
  | "ip"
  | "host"
  | "id"
  | "pii"
  | "client"
  | "user"
  | "custom";

export type Segment =
  | { kind: "plain"; text: string }
  | { kind: "redacted"; category: Category; original: string; replacement: string };

export interface Review {
  format: string;
  ai_installed: boolean;
  engagement: string | null;
  segments: Segment[];
}

export interface MaskingConfig {
  id_cut: number;
  secret_keep: number;
  pii_keep: number;
}

export interface CustomTerm {
  value: string;
  replacement: string | null;
}

export interface Config {
  name: string | null;
  client: string[];
  users: string[];
  hosts: string[];
  terms: CustomTerm[];
  allow_domains: string[];
  sensitive_headers: string[];
  sensitive_keys: string[];
  masking: MaskingConfig | null;
}

export interface AppSettings {
  hotkey: string;
  active_engagement: string | null;
  review: boolean;
  keep_last: boolean;
  forget_last_after_minutes: number;
  clear_clipboard_on_cancel: boolean;
  remember: boolean;
  unload_windows: boolean;
}

export interface Engagement {
  id: string;
  name: string;
  config: Config;
}

export interface Memory {
  app_bytes: number;
  webview_bytes: number;
  webview_processes: number;
  ai_bytes: number;
  system_total: number;
  system_used: number;
}

export interface LastRedaction {
  output: string;
  format: string;
  categories: [Category, number][];
  engagement: string | null;
  age_secs: number;
}

export type ForgottenReason = "expired" | "cleared" | "freed" | "disabled";

export interface Overview {
  last: LastRedaction | null;
  forgotten: { reason: ForgottenReason; secs_ago: number } | null;
  memory: Memory;
  redactions: number;
  uptime_secs: number;
  ai_installed: boolean;
  ai_loaded: boolean;
}

/** A redacted value and the original it replaced, for the vault. */
export interface Pair {
  replacement: string;
  original: string;
}

export type RestorePart =
  | { kind: "plain"; text: string }
  | { kind: "restored"; original: string; replacement: string }
  | { kind: "ambiguous"; replacement: string; candidates: string[] };

/** An entity found by the local AI model, in UTF-16 offsets of the scanned text. */
export interface AiEntity {
  start: number;
  end: number;
  label: string;
  score: number;
  category: Category;
  replacement: string;
}

export interface Status {
  config_dir: string;
  model_dir: string;
  ai_installed: boolean;
  error: string | null;
}

export const DEFAULT_MASKING: MaskingConfig = { id_cut: 0.2, secret_keep: 0.3, pii_keep: 0.4 };

/** The backend omits empty fields; fill them in so forms can bind to them. */
export function normalizeConfig(raw: Partial<Config>): Config {
  return {
    name: raw.name ?? null,
    client: raw.client ?? [],
    users: raw.users ?? [],
    hosts: raw.hosts ?? [],
    terms: (raw.terms ?? []).map((t) => ({ value: t.value, replacement: t.replacement ?? null })),
    allow_domains: raw.allow_domains ?? [],
    sensitive_headers: raw.sensitive_headers ?? [],
    sensitive_keys: raw.sensitive_keys ?? [],
    masking: raw.masking ?? null,
  };
}

export const api = {
  getReview: () => invoke<Review | null>("get_review"),
  finishReview: (text: string, pairs: Pair[]) => invoke<void>("finish_review", { text, pairs }),
  cancelReview: () => invoke<void>("cancel_review"),
  maskText: (text: string) => invoke<string>("mask_text", { text }),
  deepScan: (text: string) => invoke<AiEntity[]>("deep_scan", { text }),
  preview: (text: string) => invoke<Review>("preview", { text }),
  getStatus: () => invoke<Status>("get_status"),
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => invoke<void>("save_settings", { settings }),
  getGlobal: async () => normalizeConfig(await invoke<Partial<Config>>("get_global")),
  saveGlobal: (config: Config) => invoke<void>("save_global", { config }),
  listEngagements: async () =>
    (await invoke<(Omit<Engagement, "config"> & { config: Partial<Config> })[]>("list_engagements")).map(
      (e) => ({ ...e, config: normalizeConfig(e.config) }),
    ),
  createEngagement: (name: string) => invoke<string>("create_engagement", { name }),
  getOverview: () => invoke<Overview>("get_overview"),
  redactNow: () => invoke<void>("redact_now"),
  restoreText: (text: string) => invoke<{ parts: RestorePart[] }>("restore_text", { text }),
  restoreClipboard: () => invoke<[number, number]>("restore_clipboard"),
  vaultInfo: () => invoke<{ profile: string; entries: number }>("vault_info"),
  forgetVault: () => invoke<void>("forget_vault"),
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  closeWindow: () => invoke<void>("close_window"),
  copyLast: () => invoke<void>("copy_last"),
  clearLast: () => invoke<void>("clear_last"),
  clearClipboard: () => invoke<void>("clear_clipboard"),
  freeMemory: () => invoke<{ released_bytes: number; memory: Memory }>("free_memory"),
  getEngagement: async (name: string) =>
    normalizeConfig(await invoke<Partial<Config>>("get_engagement", { name })),
  saveEngagement: (name: string, config: Config) =>
    invoke<void>("save_engagement", { name, config }),
  deleteEngagement: (name: string) => invoke<void>("delete_engagement", { name }),
};

/** Joins segments back into text, with revealed values shown as originals. */
export function compose(segments: (Segment & { revealed?: boolean })[]): string {
  return segments
    .map((s) => (s.kind === "plain" ? s.text : s.revealed ? s.original : s.replacement))
    .join("");
}

export const CATEGORY_LABELS: Record<Category, string> = {
  private_key: "Private key",
  jwt: "JWT",
  credential: "Credential",
  secret: "Secret",
  card: "Card",
  email: "Email",
  uuid: "UUID",
  ip: "IP",
  host: "Host",
  id: "ID",
  pii: "Personal data",
  client: "Client",
  user: "User",
  custom: "Custom",
};
