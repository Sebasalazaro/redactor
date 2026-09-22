import "../lib/theme.css";
import { mount } from "svelte";
import Review from "./Review.svelte";

export default mount(Review, { target: document.getElementById("app")! });
