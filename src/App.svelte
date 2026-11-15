<script lang="ts">
  // The window: the sidebar of projects and chats, the open chat, and the side
  // panel for its plan and documents, in the Quiet Terminal look.
  import { onMount, tick } from "svelte";
  import { homeDir } from "@tauri-apps/api/path";
  import "./app.css";
  import ChatView from "./lib/chat/ChatView.svelte";
  import Composer from "./lib/chat/Composer.svelte";
  import SidePanel from "./lib/panel/SidePanel.svelte";
  import Settings from "./lib/Settings.svelte";
  import Sidebar from "./lib/Sidebar.svelte";
  import TopBar from "./lib/TopBar.svelte";
  import {
    autotest,
    autotestReport,
    autotestWait,
    checkSetup,
    closeSession,
    listModels,
    subscribe,
    type Decision,
    type Mode,
    type WorktreeChoice,
    type SetupInfo,
  } from "./lib/ipc";
  import { loadCachedModels, saveCachedModels } from "./lib/models";
  import { AppState } from "./lib/state/app.svelte";
  import type { Chat } from "./lib/state/chat.svelte";
  import {
    applyFontSize,
    applyPanelWidth,
    applySize,
    applyTheme,
    loadPrefs,
    shownTheme,
    sizes,
    systemIsDark,
    themes,
    watchSystem,
    type Appearance,
    type Size,
  } from "./lib/themes";

  const prefs = loadPrefs();
  // Applied here as well as in the effect below, so the window never starts in the wrong colours.
  applyTheme(shownTheme(prefs.appearance, prefs.darkTheme, prefs.lightTheme, systemIsDark()));
  applySize(prefs.size);
  applyFontSize(prefs.fontSize);
  applyPanelWidth(prefs.panelWidth);

  const app = new AppState();
  app.defaultModel = prefs.model;
  app.current.modelChoice = prefs.model;
  const chat = $derived(app.current);
  const panelOpen = $derived(chat.panelOpen && chat.panelHasContent);

  // Two picked themes, and which shows: one of them always, or whichever the
  // system is in, switching the moment it does.
  let darkTheme = $state(prefs.darkTheme);
  let lightTheme = $state(prefs.lightTheme);
  let appearance = $state<Appearance>(prefs.appearance);
  let systemDark = $state(systemIsDark());
  watchSystem((dark) => (systemDark = dark));
  $effect(() => applyTheme(shownTheme(appearance, darkTheme, lightTheme, systemDark)));
  let size = $state<Size>(prefs.size);
  let fontSize = $state(prefs.fontSize);
  let setup = $state<SetupInfo | null>(null);
  let showSettings = $state(false);

  // Cmd+, (Ctrl+, on Windows) opens settings, as in most apps; Escape closes
  // them. Cmd+N starts a new chat.
  function onKey(e: KeyboardEvent) {
    if (e.key === "," && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      showSettings = !showSettings;
    } else if (e.key.toLowerCase() === "n" && (e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey) {
      // A new chat in the open chat's folder, ready to type in.
      e.preventDefault();
      showSettings = false;
      app.newChat();
    } else if (e.key === "Escape" && showSettings) {
      showSettings = false;
    }
  }
  // Opening a chat, from the sidebar or the trash, puts settings away.
  let shownChat = app.current.sessionId;
  $effect(() => {
    if (chat.sessionId !== shownChat) {
      shownChat = chat.sessionId;
      showSettings = false;
    }
  });
  // Last known list first; refreshed from Claude Code once it has been found.
  let models = $state(loadCachedModels());

  onMount(async () => {
    await subscribe((envelope) => app.route(envelope));
    app.home = await homeDir();
    await app.refresh();
    // A first chat starts where the last one was, or at home.
    app.current.projectPath = app.projects[0]?.path ?? app.home;
    setup = await checkSetup();
    if (setup.binary) void refreshModels();
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

  // Self-test: send scripted messages through this same window, answer every
  // approval, then write out what the window ended up showing. Turned on by
  // environment variables; see `autotest` in src-tauri/src/lib.rs.
  async function runSelfTest() {
    const test = await autotest();
    if (!test) return;
    app.current.projectPath = test.folder;
    const testTheme = themes.find((t) => t.id === test.theme);
    if (testTheme) {
      if (testTheme.dark) darkTheme = testTheme.id;
      else lightTheme = testTheme.id;
      appearance = testTheme.dark ? "dark" : "light";
    }
    if (test.size && (sizes as readonly string[]).includes(test.size)) {
      size = test.size as Size;
      applySize(size);
    }
    if (test.model) app.current.modelChoice = test.model;
    const started = performance.now();
    const modelsUsed: (string | null)[] = [];
    const modesAfter: string[] = [];
    let answer: Decision | null = "allow";
    let worktreeAnswer: WorktreeChoice | null = null;
    let stopAfter = 0;
    let background = false;
    // Timed by the core: a hidden window's own timers run late.
    const pause = (ms: number) => autotestWait(ms);
    /** Resolves when the chat's process has exited. Driven by the event, not a timer. */
    const exited = (chat: Chat) =>
      new Promise<void>((resolve) => {
        const stop = $effect.root(() => {
          $effect(() => {
            if (chat.status !== "exited") return;
            resolve();
            queueMicrotask(() => stop());
          });
        });
      });
    /** The nth chat in the sidebar, counting from 0 across projects. */
    const nth = (n: string) => app.projects.flatMap((p) => p.sessions)[Number(n)]?.id;

    for (const message of test.messages) {
      // Lines starting with `@` steer the test instead of being sent:
      //   @model <name>, @mode <mode>
      //   @answer <allow|always_allow|deny|none>  how approvals are answered from here on
      //   @worktree <worktree|here|none>  how the worktree card is answered
      //   @turn          wait for the open chat's next turn to end (after a move to a
      //                  worktree, the chat carries on in a new turn by itself)
      //   @moved         wait for a turn to end with the chat in its worktree
      //   @stop <ms>     press stop that long into the next message
      //   @background    send the next message without waiting for its answer
      //   @new           start a new chat in the same folder
      //   @open <n>      open the nth chat in the sidebar (from 0)
      //   @delete <n>    delete the nth chat in the sidebar
      //   @close         shut the open chat's process down, as going idle does
      //   @exited        wait until the open chat's process has exited (idle shutdown)
      //   @endside       end the open chat's side chat
      //   A message starting with /btw goes to the side chat; the test waits for its answer.
      //   @wait <ms>     do nothing for a while. With the window hidden or the screen
      //                  locked, the page is frozen while nothing happens and this never
      //                  ends; wait for an event (like @exited) instead.
      const [, directive, value = ""] =
        message.match(/^@(model|mode|answer|worktree|turn|moved|stop|background|new|open|delete|close|exited|wait|endside)(?:\s+(\S+))?$/) ??
        [];
      const current = app.current;
      if (directive === "model") await current.changeModel(value);
      if (directive === "mode") await current.changeMode(value as Mode);
      if (directive === "answer") answer = value === "none" ? null : (value as Decision);
      if (directive === "worktree") worktreeAnswer = value === "none" ? null : (value as WorktreeChoice);
      if (directive === "moved") {
        await new Promise<void>((resolve) => {
          current.onTurnEnd = () => {
            if (current.workPath) resolve();
          };
        });
        current.onTurnEnd = null;
      }
      if (directive === "turn") {
        await new Promise<void>((resolve) => (current.onTurnEnd = resolve));
        current.onTurnEnd = null;
      }
      if (directive === "stop") stopAfter = Number(value);
      if (directive === "background") background = true;
      if (directive === "new") app.newChat();
      if (directive === "open") await app.open(nth(value) ?? "");
      if (directive === "delete") await app.remove(nth(value) ?? "");
      if (directive === "wait") await pause(Number(value));
      if (directive === "endside") await current.endSide();
      if (directive === "close") await closeSession(current.sessionId);
      if (directive === "close" || directive === "exited") await exited(current);
      if (directive) {
        await tick();
        continue;
      }

      current.autoAnswer = answer;
      current.autoWorktree = worktreeAnswer;
      // A side question ends a turn in the side chat, not the main one.
      const target = /^\/btw\s/.test(message) ? current.openSide() : current;
      const done = new Promise<void>((resolve) => (target.onTurnEnd = resolve));
      await current.send(message);
      if (stopAfter) void pause(stopAfter).then(() => current.stop());
      stopAfter = 0;
      if (background) {
        background = false;
        continue;
      }
      await done;
      target.onTurnEnd = null;
      await tick();
      modelsUsed.push(current.model);
      modesAfter.push(current.mode);
    }
    await app.refresh();
    await tick();
    const current = app.current;
    await autotestReport(
      JSON.stringify(
        {
          seconds: Math.round((performance.now() - started) / 100) / 10,
          model: current.model,
          modelsUsed,
          modesAfter,
          footer: document.querySelector<HTMLElement>(".meta")?.innerText ?? "",
          sidebar: document.querySelector<HTMLElement>("aside")?.innerText ?? "",
          projects: app.projects,
          statuses: app.statuses,
          dots: app.projects.flatMap((p) => p.sessions).map((s) => [s.title, app.dot(s.id)]),
          status: current.status,
          worktree: { branch: current.branch, workPath: current.workPath, prs: current.prs },
          trash: app.trash.map((s) => s.title),
          modelsOffered: models.map((m) => m.name),
          usage: current.usage,
          panel: {
            open: current.panelOpen,
            tab: current.activeTab,
            plan: current.plan ? { pending: current.planPending, answered: current.plan.answered ?? null } : null,
            docs: current.docs.map((doc) => ({
              path: doc.path,
              closed: !!doc.closed,
              length: doc.text.length,
              start: doc.text.slice(0, 80),
            })),
          },
          items: current.items.map((item) => ({ ...item })),
          panelView: current.panelView,
          // Where the panel's left line is, in the panel and in the top bar above it.
          edges: {
            window: window.innerWidth,
            panel: document.querySelector<HTMLElement>(".panel")?.getBoundingClientRect().left ?? null,
            bar: document.querySelector<HTMLElement>(".views.open")?.getBoundingClientRect().left ?? null,
          },
          planPending: current.planPending,
          side: current.side ? current.side.items.map((item) => ({ ...item })) : null,
          sideTitle: current.side?.title ?? null,
          // `shown` leaves out blocks the browser hasn't laid out (off screen, or
          // everything while the window is hidden); `text` has them all.
          shown: document.querySelector<HTMLElement>(".content")?.innerText ?? "",
          text: document.querySelector<HTMLElement>(".content")?.textContent ?? "",
          html: document.querySelector<HTMLElement>(".content")?.innerHTML.length ?? 0,
        },
        null,
        2,
      ),
    );
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="app" class:paneled={panelOpen}>
  <Sidebar {app} onsettings={() => (showSettings = !showSettings)} />
  <div class="right">
  <TopBar {app} />
  <!-- Chat and side panel; settings are drawn over both, never the sidebar. -->
  <div class="work">
  <main>
    <!-- Each chat gets its own view, so scroll position and open folds don't leak between chats. -->
    {#key chat.sessionId}
      <ChatView {chat} problem={setup?.problem ?? null} />
      <Composer {chat} ready={!!setup?.binary} home={app.home} {models} focus={app.focusBox} />
    {/key}
  </main>
  {#if panelOpen}
    {#key chat.sessionId}
      <SidePanel {chat} />
    {/key}
  {/if}
  <!-- Drawn over the chat and panel, which stay as they are underneath. -->
  {#if showSettings}
    <Settings {app} bind:darkTheme bind:lightTheme bind:appearance bind:size bind:fontSize onclose={() => (showSettings = false)} />
  {/if}
  </div>
  </div>
</div>

<style>
  .app {
    height: 100%;
    display: flex;
  }
  /* The top bar over the chat and panel. */
  .right {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .work {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    position: relative;
  }
  main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    position: relative;
  }
  /* Too narrow for chat and panel side by side: the open panel takes the chat's place. */
  @media (max-width: 1100px) {
    .paneled main {
      display: none;
    }
  }
</style>
