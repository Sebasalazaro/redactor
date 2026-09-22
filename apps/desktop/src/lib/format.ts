// Small display helpers shared by the dashboard pages.

export function bytes(n: number): string {
  if (n >= 1024 ** 3) return `${(n / 1024 ** 3).toFixed(1)} GB`;
  if (n >= 1024 ** 2) return `${(n / 1024 ** 2).toFixed(0)} MB`;
  if (n >= 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${n} B`;
}

export function duration(secs: number): string {
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)} min`;
  return `${Math.floor(secs / 3600)} h ${Math.floor((secs % 3600) / 60)} min`;
}

export function ago(secs: number): string {
  return secs < 10 ? "just now" : `${duration(secs)} ago`;
}

const isMac = navigator.platform.toLowerCase().includes("mac");
const MAC_ORDER: [string, string][] = [
  ["Control", "⌃"],
  ["Alt", "⌥"],
  ["Shift", "⇧"],
  ["CommandOrControl", "⌘"],
];

/** `CommandOrControl+Alt+R` → `⌥⌘R` on macOS, `Ctrl+Alt+R` elsewhere. */
export function hotkey(accel: string): string {
  if (!isMac) return accel.replace("CommandOrControl", "Ctrl");
  const parts = accel.split("+");
  const mods = MAC_ORDER.filter(([name]) => parts.includes(name)).map(([, s]) => s);
  return mods.join("") + parts[parts.length - 1];
}

/** Mirrors `redactor_core::store::slugify`, for previews only. */
export function slugify(name: string): string {
  const slug = name
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 64)
    .replace(/-+$/, "");
  return slug || "engagement";
}

export function format(name: string): string {
  return name.replaceAll("_", " ");
}
