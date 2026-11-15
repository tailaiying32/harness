<script lang="ts">
  // The window: the sidebar of projects and chats, the open chat, and the side
  // panel for its plan and documents, in the Quiet Terminal look.
  import { onMount, tick } from "svelte";
  import { homeDir } from "@tauri-apps/api/path";
  import "./app.css";
  import ChatView from "./lib/chat/ChatView.svelte";
  import Composer from "./lib/chat/Composer.svelte";
  import SidePanel from "./lib/panel/SidePanel.svelte";
  import ImageViewer from "./lib/ImageViewer.svelte";
  import Settings from "./lib/Settings.svelte";
  import Sidebar from "./lib/Sidebar.svelte";
  import TopBar from "./lib/TopBar.svelte";
  import BarIcon from "./lib/BarIcon.svelte";
  import {
    autotest,
    autotestReport,
    autotestWait,
    browserNavigate,
    checkSetup,
    closeSession,
    listModels,
    subscribe,
    terminalText,
    terminalWrite,
    type Decision,
    type Mode,
    type WorktreeChoice,
    type SetupInfo,
  } from "./lib/ipc";
  import { attach } from "./lib/chat/images";
  import { loadCachedModels, saveCachedModels, startingModel } from "./lib/models";
  import { AppState } from "./lib/state/app.svelte";
  import { terminals } from "./lib/state/terminals.svelte";
  import type { Chat } from "./lib/state/chat.svelte";
  import {
    applyFontSize,
    applyPanelWidth,
    applySize,
    applyTheme,
    loadPrefs,
    savePrefs,
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
  app.defaultModel = startingModel(prefs.model, loadCachedModels());
  app.current.modelChoice = app.defaultModel;
  const chat = $derived(app.current);
  const panelOpen = $derived(chat.panelOpen && chat.panelHasContent);
  // Full screen panel: it covers the chat, never the sidebar. Hiding the
  // panel ends it, so the panel always comes back at its own width.
  const wide = $derived(panelOpen && chat.panelWide);
  $effect(() => {
    if (!panelOpen && chat.panelWide) chat.panelWide = false;
  });

  // The sidebar put away, kept across quits. Its button stays in one spot
  // (by the window buttons on a Mac) whether the sidebar shows or not.
  let sidebarHidden = $state(prefs.sidebarHidden);
  function toggleSidebar() {
    sidebarHidden = !sidebarHidden;
    savePrefs({ sidebarHidden });
  }

  /** Typing somewhere, where Escape belongs to the box. */
  function typing(target: EventTarget | null): boolean {
    const el = target as HTMLElement | null;
    return !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable);
  }

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
  // them. Cmd+N starts a new chat. Cmd+B shows or hides the sidebar. Escape
  // also ends a full screen panel, unless it belongs to a box being typed in.
  function onKey(e: KeyboardEvent) {
    // In a terminal, Ctrl keys belong to the shell (Ctrl+C, Ctrl+N, Ctrl+R...).
    if (terminals.focused() && e.ctrlKey && !e.metaKey) return;
    if (terminalKey(e)) {
      e.preventDefault();
      return;
    }
    if (e.key === "," && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      showSettings = !showSettings;
    } else if (e.key.toLowerCase() === "n" && (e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey) {
      // A new chat in the open chat's folder, ready to type in.
      e.preventDefault();
      showSettings = false;
      app.newChat();
    } else if (e.key.toLowerCase() === "b" && (e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey) {
      e.preventDefault();
      toggleSidebar();
    } else if (e.key === "Escape" && showSettings) {
      showSettings = false;
    } else if (e.key === "Escape" && wide && !chat.viewing && !typing(e.target) && !e.defaultPrevented) {
      chat.panelWide = false;
    }
  }
  /**
   * The terminal's own shortcuts, only while it has the keys: Cmd+T for a new
   * tab in the project's main folder, Cmd+Shift+[ and ] for the tab before
   * and after. Cmd+W comes from the Mac menu (see `closeKey`).
   */
  function terminalKey(e: KeyboardEvent): boolean {
    if (!terminals.focused() || !e.metaKey || e.altKey || e.ctrlKey) return false;
    const project = app.current.projectPath.trim();
    if (!e.shiftKey && e.key.toLowerCase() === "t") {
      void terminals.open(project).catch((err) => app.current.showError(String(err)));
      return true;
    }
    if (e.shiftKey && (e.code === "BracketLeft" || e.code === "BracketRight")) {
      terminals.step(project, e.code === "BracketLeft" ? -1 : 1);
      return true;
    }
    return false;
  }
  // Keys the terminal leaves to the window: the app's own Cmd shortcuts.
  terminals.keys = (e) =>
    e.metaKey &&
    !e.ctrlKey &&
    (["t", "n", ",", "w"].includes(e.key.toLowerCase()) ||
      (e.shiftKey && (e.code === "BracketLeft" || e.code === "BracketRight")));
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
  /** For the self-test: a PNG of a word in large black letters on white. */
  async function pictureOf(word: string): Promise<Blob> {
    const canvas = document.createElement("canvas");
    canvas.width = 800;
    canvas.height = 300;
    const ctx = canvas.getContext("2d")!;
    ctx.fillStyle = "#ffffff";
    ctx.fillRect(0, 0, 800, 300);
    ctx.fillStyle = "#000000";
    ctx.font = "bold 96px sans-serif";
    ctx.fillText(word, 40, 190);
    return new Promise((resolve) => canvas.toBlob((blob) => resolve(blob!), "image/png"));
  }

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
    /** What each @termclose was asked about ("python3 …"), or null when it closed at once. */
    const termAsked: (string | null)[] = [];
    let background = false;
    /** `[image n]` words of pictures attached for the next message. */
    let imageWords: string[] = [];
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
      //   @image <word>  attach a picture of <word>, as a paste would, to the next message
      //   @file <path>   add the file at <path>, as a drop would, to the next message
      //   @browser <url> open the browser view and type <url> in its address line;
      //                  waits for the page to load. `@browser` alone only shows the view.
      //   @settings      open or close settings (drawn over the panel, hiding the page)
      //   @terminal <text>  show the terminal (opening a tab if the project has none)
      //                  and type <text> and Enter in the tab on show
      //   @termwait <text>  wait (up to 20 s) until the tab on show has <text> in it
      //   @termtab       open another tab in the project's main folder
      //   @termclose     close the tab on show, answering "close" if it asks
      //   @docs          show the docs view (the terminal goes off screen)
      //   @notes <text>  write the open chat's notes (the rest of the line, spaces and all)
      //   @planedit <text>  add a line to the next plan before it is auto-approved
      //   @wait <ms>     do nothing for a while. With the window hidden or the screen
      //                  locked, the page is frozen while nothing happens and this never
      //                  ends; wait for an event (like @exited) instead.
      const [, directive, value = ""] =
        message.match(/^@(model|mode|answer|worktree|turn|moved|stop|background|new|open|delete|close|exited|wait|endside|image|file|browser|settings|termtab|termclose|docs)(?:\s+(\S+))?$/) ??
        message.match(/^@(notes|planedit|terminal|termwait)\s+(.+)$/s) ??
        [];
      const current = app.current;
      if (directive === "notes") await current.saveNotes(value);
      if (directive === "planedit") current.autoPlanEdit = value;
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
      if (directive === "image") imageWords.push(await attach(current, await pictureOf(value)));
      if (directive === "file") imageWords.push(await current.addDraftFile(value));
      if (directive === "browser") {
        current.openBrowser();
        if (value) {
          await browserNavigate(current.sessionId, value);
          await pause(300);
          for (let i = 0; i < 100 && (!current.browser || current.browser.loading); i++) await pause(100);
        }
      }
      if (directive === "settings") showSettings = !showSettings;
      const project = current.projectPath.trim();
      if (directive === "terminal") {
        current.showView("terminal");
        for (let i = 0; i < 100 && !terminals.shown(project); i++) await pause(100);
        const tab = terminals.shown(project);
        if (tab) await terminalWrite(tab.id, `${value}\r`);
        await pause(300);
      }
      if (directive === "termwait") {
        for (let i = 0; i < 100; i++) {
          const tab = terminals.shown(project);
          const text = tab ? await terminalText(tab.id, 200).catch(() => "") : "";
          if (text.includes(value)) break;
          await pause(200);
        }
      }
      if (directive === "termtab") await terminals.open(project);
      if (directive === "termclose") {
        const tab = terminals.shown(project);
        if (tab) {
          await terminals.askClose(tab.id);
          termAsked.push(terminals.confirming?.running ?? null);
          if (terminals.confirming) await terminals.close(terminals.confirming.id);
          await pause(300);
        }
      }
      if (directive === "docs") current.showView("docs");
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
      await current.send([...imageWords, message].join(" "));
      imageWords = [];
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
    const termProject = current.projectPath.trim();
    const termTab = terminals.shown(termProject);
    const termReport = {
      on: current.terminalOn,
      tabs: terminals.tabs(termProject).map((t) => ({ title: t.title, running: t.running, closed: t.closed, worktree: t.worktree })),
      asked: termAsked,
      text: termTab ? await terminalText(termTab.id, 40).catch((e) => String(e)) : null,
    };
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
          notes: current.notes,
          plans: current.items.filter((item) => item.type === "approval" && item.approval === "plan").map((item) => ({ ...item })),
          terminal: termReport,
          browser: {
            on: current.browserOn,
            page: current.browser,
            pageLines: current.items.filter((item) => item.type === "page").length,
            slot: (() => {
              const r = document.querySelector<HTMLElement>(".slot")?.getBoundingClientRect();
              return r ? { x: r.left, y: r.top, width: r.width, height: r.height } : null;
            })(),
          },
          // Where the panel's left line is, in the panel and in the top bar above it.
          edges: {
            window: window.innerWidth,
            panel: document.querySelector<HTMLElement>(".panel")?.getBoundingClientRect().left ?? null,
            bar: document.querySelector<HTMLElement>(".over-panel")?.getBoundingClientRect().left ?? null,
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

<!-- A file dropped anywhere but the message box would replace the page. -->
<svelte:window
  onkeydown={onKey}
  ondragover={(e) => e.dataTransfer?.types.includes("Files") && e.preventDefault()}
  ondrop={(e) => e.dataTransfer?.types.includes("Files") && e.preventDefault()}
/>

<div class="app" class:paneled={panelOpen} class:wide class:bare={sidebarHidden}>
  {#if !sidebarHidden}
    <Sidebar {app} onsettings={() => (showSettings = !showSettings)} />
  {/if}
  <button class="side-toggle" title={sidebarHidden ? "Show the sidebar (⌘B)" : "Hide the sidebar (⌘B)"} onclick={toggleSidebar}>
    <BarIcon name="sidebar" />
  </button>
  <div class="right">
  <TopBar {app} />
  <!-- Chat and side panel; settings are drawn over both, never the sidebar. -->
  <div class="work">
  <main>
    <!-- Each chat gets its own view, so scroll position and open folds don't leak between chats. -->
    {#key chat.sessionId}
      <ChatView {chat} problem={setup?.problem ?? null} />
      <Composer {chat} ready={!!setup?.binary} home={app.home} {models} focus={app.focusBox} drops={!showSettings && !chat.viewing} />
    {/key}
  </main>
  {#if panelOpen}
    {#key chat.sessionId}
      <SidePanel {chat} covered={showSettings || !!chat.viewing} />
    {/key}
  {/if}
  <!-- Drawn over the chat and panel, which stay as they are underneath. -->
  {#if chat.viewing}
    {#key chat.sessionId}
      <ImageViewer {chat} />
    {/key}
  {/if}
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
    position: relative;
  }
  /* In the top strip. With the sidebar showing: at its right edge, in line
     with the end of its rows (the sidebar is --side plus its side padding).
     Hidden: at the top bar's start (the top bar leaves room, see TopBar). */
  .side-toggle {
    position: absolute;
    top: 0;
    left: calc(var(--side) + var(--pad-x) * 0.6 - 1em);
    height: var(--bar);
    display: flex;
    align-items: center;
    color: var(--faint);
    z-index: 2;
  }
  .bare .side-toggle {
    left: var(--toggle-x);
  }
  .side-toggle:hover {
    color: var(--fg);
  }
  /* Full screen panel: the chat steps aside, as in a narrow window. */
  .wide main {
    display: none;
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
