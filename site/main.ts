// The website's one page. See PLAN.md, "Website".
import { mount } from "svelte";
import "./site.css";
import Page from "./Page.svelte";
import { show, shownTheme } from "./look";

show(shownTheme());
mount(Page, { target: document.getElementById("page")! });
