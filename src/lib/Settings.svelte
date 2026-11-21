<script lang="ts">
  // Settings, drawn over everything but the sidebar: top bar, chat and side panel. A choice applies at once and
  // is remembered. Also home to things that aren't part of the work at hand,
  // like the trash.
  import BarIcon from "./BarIcon.svelte";
  import FindBar from "./FindBar.svelte";
  import Preferences from "./Preferences.svelte";
  import { refreshSkills } from "./commands.svelte";
  import { closeFind, domSearcher } from "./find";
  import { AGENTS, ANTIGRAVITY, CLAUDE, CODEX, agentLabel, importSkill, listSkills, pickFile, pickFolder, removeSkill, type AgentId, type ModelInfo, type SkillInfo } from "./ipc";
  import type { AgentModels } from "./models";
  import type { AppState, Limit } from "./state/app.svelte";
  import {
    appearances,
    applyCommentButton,
    applyFontSize,
    applySize,
    fontSizes,
    loadPrefs,
    overnightTimes,
    savePrefs,
    sizes,
    themes,
    type Appearance,
    type Size,
  } from "./themes";

  let {
    app,
    darkTheme = $bindable(),
    lightTheme = $bindable(),
    appearance = $bindable(),
    size = $bindable(),
    fontSize = $bindable(),
    models = { "claude-code": [], codex: [], antigravity: [], cursor: [] },
    shownModels = { "claude-code": [], codex: [], antigravity: [], cursor: [] },
    onModelPick = () => {},
    onclose,
  }: {
    app: AppState;
    darkTheme: string;
    lightTheme: string;
    appearance: Appearance;
    size: Size;
    fontSize: number;
    /** Every model each agent offers. */
    models?: AgentModels;
    /** Which model values each agent shows in the model menu. */
    shownModels?: Record<AgentId, string[]>;
    onModelPick?: (agent: AgentId, value: string, on: boolean) => void;
    onclose: () => void;
  } = $props();

  // Models: one row per installed agent with a floating dropdown menu of
  // checkboxes. Long lists get a filter box and pin the models already on at
  // the top when opened so toggling doesn't make rows jump under the pointer.
  const FILTER_AT = 8;
  const PIN_AT = 12;
  const modelAgents = $derived(
    AGENTS.filter((agent) => (!app.agents.length || app.agents.includes(agent)) && models[agent].length > 0),
  );
  let openAgent = $state<AgentId | null>(null);
  let modelQuery = $state("");
  let pinnedOnOpen = $state<string[]>([]);
  let openPicker = $state<HTMLElement>();
  let filterBox = $state<HTMLInputElement>();

  function toggleModels(agent: AgentId) {
    if (openAgent === agent) {
      closeModels();
      return;
    }
    openAgent = agent;
    modelQuery = "";
    pinnedOnOpen = [...shownModels[agent]];
  }

  function closeModels() {
    openAgent = null;
    modelQuery = "";
  }

  function shownCount(agent: AgentId): number {
    const shown = shownModels[agent];
    return models[agent].filter((m) => shown.includes(m.value)).length;
  }

  function toggleModel(agent: AgentId, value: string) {
    const on = shownModels[agent].includes(value);
    if (on && shownCount(agent) <= 1) return;
    onModelPick(agent, value, !on);
  }

  /** Filtered models for the open dropdown, split into pinned-on-open + rest for long lists. */
  const openModels = $derived.by(() => {
    if (!openAgent) return { top: [] as ModelInfo[], rest: [] as ModelInfo[] };
    const list = models[openAgent];
    const words = modelQuery.toLowerCase().split(/\s+/).filter(Boolean);
    if (words.length) {
      const matched = list.filter((m) => words.every((w) => `${m.name} ${m.value}`.toLowerCase().includes(w)));
      return { top: matched, rest: [] as ModelInfo[] };
    }
    if (list.length > PIN_AT) {
      const top = list.filter((m) => pinnedOnOpen.includes(m.value));
      const rest = list.filter((m) => !pinnedOnOpen.includes(m.value));
      return { top, rest };
    }
    return { top: list, rest: [] as ModelInfo[] };
  });

  function filterKey(e: KeyboardEvent) {
    if (e.key === "Enter" && openAgent && modelQuery.trim() && openModels.top.length) {
      e.preventDefault();
      toggleModel(openAgent, openModels.top[0].value);
    }
  }

  // Click outside closes the open model dropdown; opening a filtered list puts the cursor in its box.
  $effect(() => {
    if (!openAgent) return;
    filterBox?.focus();
    const away = (e: PointerEvent) => {
      if (!openPicker?.contains(e.target as Node)) closeModels();
    };
    window.addEventListener("pointerdown", away, true);
    return () => window.removeEventListener("pointerdown", away, true);
  });

  // One pick per side. The app shows whichever side the appearance says, so
  // picking for the other side changes nothing on screen. App.svelte applies it.
  const groups = $derived([
    { name: "dark", picked: darkTheme, list: themes.filter((t) => t.dark) },
    { name: "light", picked: lightTheme, list: themes.filter((t) => !t.dark) },
  ]);

  function pickTheme(id: string, dark: boolean) {
    if (dark) darkTheme = id;
    else lightTheme = id;
    savePrefs(dark ? { darkTheme: id } : { lightTheme: id });
  }

  function pickAppearance(value: Appearance) {
    appearance = value;
    savePrefs({ appearance: value });
  }

  function pickSize(value: Size) {
    size = value;
    applySize(value);
    savePrefs({ size: value });
  }

  // Claude Code's limits, by the names it reports them under (CLI 2.1.326):
  // "seven_day" covers all models and "seven_day_overage_included" is Fable's
  // own, matched against `/usage`; "five_hour", "seven_day_opus" and
  // "seven_day_sonnet" show when the plan has them, and any other by its own
  // name. "all models" and "fable" always show. Each agent gets its own heading.
  const CLAUDE_WINDOWS: [string, string][] = [
    ["five_hour", "5h"],
    ["seven_day", "all models"],
    ["seven_day_opus", "opus"],
    ["seven_day_sonnet", "sonnet"],
    ["seven_day_overage_included", "fable"],
  ];
  const ALWAYS = new Set(["seven_day", "seven_day_overage_included"]);
  const claudeLimits = $derived.by(() => {
    const read = app.limits[CLAUDE] ?? {};
    const known = CLAUDE_WINDOWS.filter(([window]) => ALWAYS.has(window) || read[window]);
    const others = Object.keys(read)
      .filter((window) => !CLAUDE_WINDOWS.some(([w]) => w === window))
      .map((window): [string, string] => [window, window.replaceAll("_", " ")]);
    return [...known, ...others].map(([window, name]) => ({ name, text: limitText(read[window]) }));
  });

  // Codex's and Antigravity's limits, as many as the plan has, by the names
  // the app gives them: Codex's by window ("5h", "week"), Antigravity's by
  // model group and window ("gemini 5h", "claude and gpt week").
  const agentLimits = (agent: AgentId) =>
    Object.entries(app.limits[agent] ?? {}).map(([name, limit]) => ({ name, text: limitText(limit) }));
  const otherLimits = $derived(
    [CODEX, ANTIGRAVITY]
      .filter((agent) => app.agents.includes(agent))
      .map((agent) => ({ agent, label: agentLabel(agent), limits: agentLimits(agent) })),
  );

  /** "64% · resets Sun 4:59 PM"; unknown before any reading. Once the limit has
   *  started over it reads 0% (use elsewhere since then isn't known) until the
   *  next reading, which also brings the next reset time. */
  function limitText(limit: Limit | undefined): string {
    if (!limit) return "unknown";
    if (limit.resetsAt !== null && limit.resetsAt * 1000 < Date.now()) return "0%";
    const percent = `${Math.round(limit.utilization * 100)}%`;
    if (limit.resetsAt === null) return percent;
    const when = new Date(limit.resetsAt * 1000).toLocaleString([], { weekday: "short", hour: "numeric", minute: "2-digit" });
    return `${percent} · resets ${when}`;
  }

  /** How long a trashed chat has left before it is deleted for good. */
  const TRASH_DAYS = 30;
  function daysLeft(trashedAt: number | null): string {
    const left = Math.ceil(TRASH_DAYS - (Date.now() - (trashedAt ?? Date.now())) / 86_400_000);
    return left <= 1 ? "last day" : `${left}d left`;
  }

  /** Like a sidebar project: the most recent CAP chats, then "show more" adds CAP at a time. */
  const CAP = 8;
  let trashShown = $state(CAP);

  /** "empty trash" was clicked once and is asking to be sure. */
  let emptying = $state(false);

  // When overnight keep awake ends.
  let commentButton = $state(loadPrefs().commentButton);
  function pickCommentButton(on: boolean) {
    commentButton = on;
    savePrefs({ commentButton: on });
    applyCommentButton(on);
  }

  let overnightUntil = $state(loadPrefs().overnightUntil);
  function pickOvernight(time: string) {
    overnightUntil = time;
    savePrefs({ overnightUntil: time });
  }

  async function emptyTrash() {
    emptying = false;
    await app.emptyTrash();
  }

  // The app's own skills, which every chat's agent can use. Copies: importing
  // one with the same name replaces it.
  let skills = $state<SkillInfo[]>([]);
  /** The skill whose "remove" was clicked, asking to be sure. */
  let removing = $state<string | null>(null);
  let skillProblem = $state("");

  async function loadSkills() {
    try {
      skills = await listSkills();
    } catch (e) {
      skillProblem = String(e);
    }
  }
  loadSkills();

  /** After an import or a removal: this list, and the `/` menu's. */
  async function skillsChanged() {
    await loadSkills();
    void refreshSkills();
  }

  async function importFrom(pick: () => Promise<string | null>) {
    skillProblem = "";
    removing = null;
    try {
      const path = await pick();
      if (!path) return;
      await importSkill(path);
      await skillsChanged();
    } catch (e) {
      skillProblem = String(e);
    }
  }

  async function removeOne(name: string) {
    skillProblem = "";
    removing = null;
    try {
      await removeSkill(name);
      await skillsChanged();
    } catch (e) {
      skillProblem = String(e);
    }
  }

  /** A page within Settings, in place of the list; null for the list. */
  let page = $state<"preferences" | null>(null);
  let shell = $state<HTMLElement>();
  let scroller = $state<HTMLElement>();
  let prefsView = $state<Preferences>();

  /** Escape closes an open find box or model dropdown first; on a page it goes
   *  back to the list, not out of Settings. Caught before the app's own key handler. */
  function pageKey(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    if (closeFind()) {
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    if (openAgent) {
      e.preventDefault();
      e.stopPropagation();
      if (modelQuery) modelQuery = "";
      else closeModels();
      return;
    }
    if (page) {
      e.stopPropagation();
      page = null;
    }
  }

  function pickFontSize(px: number) {
    fontSize = px;
    applyFontSize(px);
    savePrefs({ fontSize: px });
  }
</script>

<svelte:window onkeydowncapture={pageKey} />

<!-- It covers the title bar's strip, so its empty space moves the window. -->
<div class="settings-shell" bind:this={shell}>
  {#key page}
    <FindBar
      root={shell}
      modal
      searcher={() => (page === "preferences" ? (prefsView?.searcher() ?? null) : scroller ? domSearcher(scroller) : null)}
    />
  {/key}
<div class="settings" bind:this={scroller} data-tauri-drag-region>
  <div class="inner">
    <div class="head">
      {#if page}
        <span class="title"><button class="back" onclick={() => (page = null)}>Settings</button> / {page}</span>
      {:else}
        <span class="title">Settings</span>
      {/if}
      <button class="close" title="Close" onclick={onclose}><BarIcon name="close" /></button>
    </div>

    {#if page === "preferences"}
      <Preferences bind:this={prefsView} />
    {:else}

    <section>
      <div class="label">Spacing</div>
      <div class="choices">
        {#each sizes as s (s)}
          <button class:on={s === size} onclick={() => pickSize(s)}>
            <span class="mark">{s === size ? "(•)" : "( )"}</span>{s}
          </button>
        {/each}
      </div>
    </section>

    <section>
      <div class="label">Font size</div>
      <div class="choices">
        {#each fontSizes as px (px)}
          <button class:on={px === fontSize} onclick={() => pickFontSize(px)}>
            <span class="mark">{px === fontSize ? "(•)" : "( )"}</span>{px}
          </button>
        {/each}
      </div>
    </section>

    <section>
      <div class="label">Appearance</div>
      <div class="choices">
        {#each appearances as a (a)}
          <button class:on={a === appearance} onclick={() => pickAppearance(a)}>
            <span class="mark">{a === appearance ? "(•)" : "( )"}</span>{a}
          </button>
        {/each}
      </div>
    </section>

    <section>
      <div class="label">Theme</div>
      {#each groups as group (group.name)}
        <div class="sub">{group.name}</div>
        <div class="choices grid">
          {#each group.list as t (t.id)}
            <button class:on={t.id === group.picked} onclick={() => pickTheme(t.id, t.dark)}>
              <span class="mark">{t.id === group.picked ? "(•)" : "( )"}</span>{t.name}
            </button>
          {/each}
        </div>
      {/each}
    </section>

    <section>
      <div class="label">Comment button</div>
      <div class="choices">
        {#each [true, false] as on (on)}
          <button class:on={on === commentButton} onclick={() => pickCommentButton(on)}>
            <span class="mark">{on === commentButton ? "(•)" : "( )"}</span>{on ? "show" : "hide"}
          </button>
        {/each}
      </div>
    </section>

    {#if modelAgents.length}
      <section>
        <div class="label">Models</div>
        {#each modelAgents as agent (agent)}
          {@const count = shownCount(agent)}
          {@const total = models[agent].length}
          {@const isOpen = openAgent === agent}
          <div class="model-picker">
            {#if isOpen}
              <div bind:this={openPicker}>
                <button
                  class="model-trigger open"
                  aria-haspopup="menu"
                  aria-expanded="true"
                  onclick={() => toggleModels(agent)}
                >
                  <span class="name">{agentLabel(agent)}</span>
                  <span class="count">{count} of {total} ▾</span>
                </button>
                <div class="model-menu" role="menu">
                  {#if total > FILTER_AT}
                    <input
                      class="model-filter"
                      placeholder="filter"
                      spellcheck="false"
                      bind:this={filterBox}
                      bind:value={modelQuery}
                      onkeydown={filterKey}
                    />
                  {/if}
                  <div class="model-list">
                    {#each openModels.top as m (m.value)}
                      {@const on = shownModels[agent].includes(m.value)}
                      <button
                        class="model-item"
                        class:on
                        role="menuitemcheckbox"
                        aria-checked={on}
                        disabled={on && count <= 1}
                        onclick={() => toggleModel(agent, m.value)}
                      >
                        <span class="mark">{on ? "[x]" : "[ ]"}</span><span class="model-name">{m.name}</span>
                      </button>
                    {/each}
                    {#if openModels.top.length && openModels.rest.length}
                      <div class="model-sep"></div>
                    {/if}
                    {#each openModels.rest as m (m.value)}
                      {@const on = shownModels[agent].includes(m.value)}
                      <button
                        class="model-item"
                        class:on
                        role="menuitemcheckbox"
                        aria-checked={on}
                        disabled={on && count <= 1}
                        onclick={() => toggleModel(agent, m.value)}
                      >
                        <span class="mark">{on ? "[x]" : "[ ]"}</span><span class="model-name">{m.name}</span>
                      </button>
                    {/each}
                    {#if modelQuery.trim() && !openModels.top.length}
                      <div class="model-empty">no match</div>
                    {/if}
                  </div>
                </div>
              </div>
            {:else}
              <button
                class="model-trigger"
                aria-haspopup="menu"
                aria-expanded="false"
                onclick={() => toggleModels(agent)}
              >
                <span class="name">{agentLabel(agent)}</span>
                <span class="count">{count} of {total} ▾</span>
              </button>
            {/if}
          </div>
        {/each}
      </section>
    {/if}

    <section>
      <div class="label">Agents</div>
      <div class="row">
        <span class="name">preferences</span>
        <button class="act" onclick={() => (page = "preferences")}>edit</button>
      </div>
    </section>

    <section>
      <div class="label">Skills</div>
      {#each skills as skill (skill.name)}
        <div class="row">
          <span class="skill">{skill.name}</span>
          {#if removing === skill.name}
            <span class="warn">Remove {skill.name}?</span>
            <button class="danger" onclick={() => removeOne(skill.name)}>yes</button>
            <button class="act" onclick={() => (removing = null)}>no</button>
          {:else}
            <span class="about" title={skill.description}>{skill.description}</span>
            <button class="act" onclick={() => (removing = skill.name)}>remove</button>
          {/if}
        </div>
      {/each}
      <div class="empty">
        <button class="act" onclick={() => importFrom(() => pickFolder())}>import folder</button>
        <button class="act" onclick={() => importFrom(() => pickFile("Skill", ["zip", "md"]))}>import file</button>
      </div>
      {#if skillProblem}
        <div class="sub">{skillProblem}</div>
      {/if}
    </section>

    <section>
      <div class="label">Usage</div>
      <div class="sub">Claude Code</div>
      {#each claudeLimits as limit (limit.name)}
        <div class="row">
          <span class="name">{limit.name}</span>
          <span class="left">{limit.text}</span>
        </div>
      {/each}
      {#each otherLimits as { agent, label, limits } (agent)}
        <div class="sub">{label}</div>
        {#each limits as limit (limit.name)}
          <div class="row">
            <span class="name">{limit.name}</span>
            <span class="left">{limit.text}</span>
          </div>
        {:else}
          <div class="row"><span class="name">all models</span><span class="left">unknown</span></div>
        {/each}
      {/each}
    </section>

    <section>
      <div class="label">Overnight keep awake until</div>
      <div class="choices">
        {#each overnightTimes as time (time)}
          <button class:on={time === overnightUntil} onclick={() => pickOvernight(time)}>
            <span class="mark">{time === overnightUntil ? "(•)" : "( )"}</span>{time.replace(/^0/, "")}
          </button>
        {/each}
      </div>
    </section>

    <section>
      <div class="label">Trash</div>
      {#if app.trash.length === 0}
        <div class="sub">empty</div>
      {:else}
        <div class="trash">
          {#each app.trash.slice(0, trashShown) as session (session.id)}
            <div class="row">
              <button class="open" onclick={() => app.open(session.id)} title={session.title}>
                {session.title || "untitled"}
              </button>
              <span class="left">{daysLeft(session.trashed_at)}</span>
              <button class="act" onclick={() => app.restore(session.id)}>restore</button>
            </div>
          {/each}
          {#if trashShown < app.trash.length}
            <button class="more" onclick={() => (trashShown += CAP)}>show more</button>
          {/if}
        </div>
        <div class="empty">
          {#if emptying}
            <span class="warn">Delete {app.trash.length} {app.trash.length === 1 ? "chat" : "chats"} for good?</span>
            <button class="danger" onclick={emptyTrash}>yes</button>
            <button class="act" onclick={() => (emptying = false)}>no</button>
          {:else}
            <button class="act" onclick={() => (emptying = true)}>empty trash</button>
          {/if}
        </div>
      {/if}
    </section>
    {/if}
  </div>
</div>
</div>

<style>
  .settings-shell {
    position: absolute;
    inset: 0;
    /* Above the side panel's drag edge. */
    z-index: 2;
    background: var(--bg);
    display: flex;
    flex-direction: column;
    --find-right: max(var(--pad-x), calc((100% - var(--col)) / 2));
  }
  .settings {
    flex: 1;
    min-height: 0;
    /* Always on so the column doesn't jump when content grows past the fold. */
    overflow-y: scroll;
    padding: var(--pad-y) var(--pad-x);
  }
  .inner {
    max-width: var(--col);
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: calc(var(--gap) * 2);
  }
  .head {
    display: flex;
    justify-content: space-between;
    padding-bottom: var(--gap);
    border-bottom: 1px solid var(--line);
  }
  .title {
    color: var(--accent);
    font-weight: 700;
  }
  /* The panel's close icon, so every close in the app looks the same. */
  .head button {
    display: flex;
    align-items: center;
    color: var(--faint);
  }
  .head button:hover {
    color: var(--fg);
  }
  .head .title {
    display: flex;
    gap: 1ch;
  }
  .head .back {
    color: var(--accent);
    font-weight: 700;
  }
  .label {
    color: var(--bright);
    font-weight: 700;
    margin-bottom: calc(var(--gap) * 0.6);
  }
  .sub {
    color: var(--faint);
    margin-top: calc(var(--gap) * 0.8);
  }
  .choices {
    display: flex;
    flex-wrap: wrap;
    gap: 0 4ch;
  }
  .choices.grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(24ch, 1fr));
    gap: 0 2ch;
  }
  .choices button {
    text-align: left;
    color: var(--fg);
  }
  .choices button:hover,
  .choices button.on {
    color: var(--bright);
  }
  .mark {
    color: var(--faint);
    margin-right: 1ch;
    white-space: pre;
  }
  .on .mark {
    color: var(--accent);
  }
  .trash {
    margin-top: calc(var(--gap) * 0.8);
  }
  .row {
    display: flex;
    align-items: baseline;
    gap: 2ch;
  }
  .open {
    flex: 1;
    min-width: 0;
    text-align: left;
    color: var(--dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .open:hover {
    color: var(--fg);
  }
  .left {
    flex-shrink: 0;
    color: var(--faint);
  }
  .act {
    flex-shrink: 0;
    color: var(--faint);
  }
  .act:hover {
    color: var(--fg);
  }
  .more {
    color: var(--faint);
  }
  .more:hover {
    color: var(--fg);
  }
  .empty {
    display: flex;
    gap: 2ch;
    margin-top: calc(var(--gap) * 0.8);
  }
  .warn {
    color: var(--fg);
  }
  .skill {
    flex-shrink: 0;
    color: var(--fg);
  }
  .about {
    flex: 1;
    min-width: 0;
    color: var(--faint);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row .warn {
    flex: 1;
  }
  .name {
    flex: 1;
    min-width: 0;
    color: var(--fg);
  }
  .model-picker {
    position: relative;
  }
  .model-trigger {
    display: flex;
    align-items: baseline;
    gap: 2ch;
    width: 100%;
    text-align: left;
  }
  .model-trigger:hover .name,
  .model-trigger.open .name {
    color: var(--bright);
  }
  .model-trigger .count {
    flex-shrink: 0;
    color: var(--faint);
  }
  .model-trigger:hover .count,
  .model-trigger.open .count {
    color: var(--fg);
  }
  .model-menu {
    position: absolute;
    top: calc(100% + var(--gap) * 0.3);
    left: 0;
    right: 0;
    z-index: 10;
    display: flex;
    flex-direction: column;
    max-height: min(48vh, 22em);
    background: var(--panel);
    border: 1px solid var(--line);
  }
  .model-filter {
    flex-shrink: 0;
    box-sizing: border-box;
    width: 100%;
    padding: calc(var(--gap) * 0.4) 2ch;
    font: inherit;
    color: var(--fg);
    background: none;
    border: none;
    border-bottom: 1px solid var(--line);
    outline: none;
  }
  .model-filter::placeholder {
    color: var(--faint);
  }
  .model-filter:focus {
    border-bottom-color: var(--accent);
  }
  .model-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    padding: calc(var(--gap) * 0.4) 0;
  }
  .model-item {
    display: flex;
    align-items: baseline;
    width: 100%;
    padding: 0 2ch;
    text-align: left;
    color: var(--dim);
  }
  .model-item.on {
    color: var(--fg);
  }
  .model-item:hover:not(:disabled) {
    color: var(--bright);
    background: var(--line);
  }
  .model-item:disabled {
    cursor: default;
  }
  .model-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .model-sep {
    margin: calc(var(--gap) * 0.4) 0;
    border-top: 1px solid var(--line);
  }
  .model-empty {
    padding: 0 2ch;
    color: var(--faint);
  }
  .danger {
    color: var(--bad);
  }
</style>
