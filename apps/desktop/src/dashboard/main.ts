import "../lib/theme.css";
import { mount } from "svelte";
import Dashboard from "./Dashboard.svelte";

// In a plain browser during development, fake the backend (src/lib/devmock.ts).
if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) await import("../lib/devmock");

export default mount(Dashboard, { target: document.getElementById("app")! });
