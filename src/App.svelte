<script lang="ts">
  // One chat in the Quiet Terminal look. The sidebar, saved history and the
  // side panel come in later steps of PLAN.md.
  import { onMount, tick } from "svelte";
  import { homeDir } from "@tauri-apps/api/path";
  import "./app.css";
  import ChatView from "./lib/chat/ChatView.svelte";
  import Composer from "./lib/chat/Composer.svelte";
  import {
    autotest,
    autotestReport,
    checkSetup,
    listModels,
    subscribe,
    type Decision,
    type Mode,
    type SetupInfo,
  } from "./lib/ipc";
  import { loadCachedModels, saveCachedModels } from "./lib/models";
  import { Chat } from "./lib/state/chat.svelte";
  import { applySize, applyTheme, loadPrefs, sizes, type Size } from "./lib/themes";

  const prefs = loadPrefs();
  applyTheme(prefs.theme);
  applySize(prefs.size);

  const chat = new Chat();
  chat.modelChoice = prefs.model;
  let theme = $state(prefs.theme);
  let size = $state<Size>(prefs.size);
  let setup = $state<SetupInfo | null>(null);
  let home = $state("");
  // Last known list first; refreshed from Claude Code once it has been found.
  let models = $state(loadCachedModels());

  onMount(async () => {
    await subscribe((envelope) => {
      if (envelope.session_id === chat.sessionId) chat.handle(envelope.event);
    });
    home = await homeDir();
    chat.projectPath = home;
    setup = await checkSetup();
    if (setup.logged_in) void refreshModels();
    void runSelfTest();
  });

  async function refreshModels() {
    try {
      const list = await listModels();
      if (list.length > 0) {
        models = list;
        saveCachedModels(list);
      }
    } catch {
      // Keep the list we have; the picker still works with it.
    }
  }

  // Self-test: send scripted messages through this same window, allow every
  // approval, then write out what the window ended up showing. Turned on by
  // environment variables; see `autotest` in src-tauri/src/lib.rs.
  async function runSelfTest() {
    const test = await autotest();
    if (!test) return;
    chat.autoAnswer = "allow";
    chat.projectPath = test.folder;
    if (test.theme) {
      theme = test.theme;
      applyTheme(test.theme);
    }
    if (test.size && (sizes as readonly string[]).includes(test.size)) {
      size = test.size as Size;
      applySize(size);
    }
    if (test.model) chat.modelChoice = test.model;
    const started = performance.now();
    const modelsUsed: (string | null)[] = [];
    const modesAfter: string[] = [];
    let stopAfter = 0;
    for (const message of test.messages) {
      // Lines starting with `@` steer the test instead of being sent:
      // `@model <name>`, `@mode <mode>`, `@answer <allow|always_allow|deny|none>`
      // (how approvals are answered from here on), and `@stop <ms>` (press
      // stop that long into the next message).
      const [, directive, value] = message.match(/^@(model|mode|answer|stop)\s+(\S+)$/) ?? [];
      if (directive === "model") await chat.changeModel(value);
      if (directive === "mode") await chat.changeMode(value as Mode);
      if (directive === "answer") chat.autoAnswer = value === "none" ? null : (value as Decision);
      if (directive === "stop") stopAfter = Number(value);
      if (directive) continue;

      const done = new Promise<void>((resolve) => (chat.onTurnEnd = resolve));
      await chat.send(message);
      if (stopAfter) setTimeout(() => void chat.stop(), stopAfter);
      stopAfter = 0;
      await done;
      await tick();
      modelsUsed.push(chat.model);
      modesAfter.push(chat.mode);
    }
    chat.onTurnEnd = null;
    await tick();
    await autotestReport(
      JSON.stringify(
        {
          seconds: Math.round((performance.now() - started) / 100) / 10,
          model: chat.model,
          modelsUsed,
          modesAfter,
          footer: document.querySelector<HTMLElement>(".meta")?.innerText ?? "",
          modelsOffered: models.map((m) => m.name),
          usage: chat.usage,
          items: chat.items.map((item) => ({ ...item })),
          shown: document.querySelector<HTMLElement>(".content")?.innerText ?? "",
          html: document.querySelector<HTMLElement>(".content")?.innerHTML.length ?? 0,
        },
        null,
        2,
      ),
    );
  }
</script>

<main>
  <ChatView {chat} problem={setup?.problem ?? null} />
  <Composer {chat} ready={!!setup?.logged_in} {home} {models} bind:theme bind:size />
</main>

<style>
  main {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
</style>
