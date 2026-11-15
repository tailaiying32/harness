import { mount } from "svelte";
import App from "./App.svelte";

// On a Mac the window draws into its own title bar (see tauri.conf.json), and
// the sidebar leaves room at its top for the window buttons.
if (navigator.userAgent.includes("Mac")) document.documentElement.classList.add("mac");

mount(App, { target: document.getElementById("app")! });
