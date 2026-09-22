import "../lib/theme.css";
import { mount } from "svelte";
import Dashboard from "./Dashboard.svelte";

export default mount(Dashboard, { target: document.getElementById("app")! });
