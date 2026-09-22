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
}

export interface Status {
  config_dir: string;
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
  finishReview: (text: string) => invoke<void>("finish_review", { text }),
  cancelReview: () => invoke<void>("cancel_review"),
  maskText: (text: string) => invoke<string>("mask_text", { text }),
  preview: (text: string) => invoke<Review>("preview", { text }),
  getStatus: () => invoke<Status>("get_status"),
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => invoke<void>("save_settings", { settings }),
  getGlobal: async () => normalizeConfig(await invoke<Partial<Config>>("get_global")),
  saveGlobal: (config: Config) => invoke<void>("save_global", { config }),
  listEngagements: () => invoke<string[]>("list_engagements"),
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
