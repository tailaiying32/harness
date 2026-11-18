<script lang="ts">
  // Settings, drawn over everything but the sidebar: top bar, chat and side panel. A choice applies at once and
  // is remembered. Also home to things that aren't part of the work at hand,
  // like the trash.
  import BarIcon from "./BarIcon.svelte";
  import Preferences from "./Preferences.svelte";
  import { refreshSkills } from "./commands.svelte";
  import { CLAUDE, CODEX, importSkill, listSkills, pickFile, pickFolder, removeSkill, type ModelInfo, type SkillInfo } from "./ipc";
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
    cursorModels = [],
    cursorShown = [],
    onCursorPick = () => {},
    onclose,
  }: {
    app: AppState;
    darkTheme: string;
    lightTheme: string;
    appearance: Appearance;
    size: Size;
    fontSize: number;
    /** Every model Cursor offers; empty when Cursor isn't installed. */
    cursorModels?: ModelInfo[];
    /** The ones the model menu shows. */
    cursorShown?: string[];
    onCursorPick?: (value: string, on: boolean) => void;
    onclose: () => void;
  } = $props();

  // Cursor's models: the ones in the menu, each with "remove", and one box
  // that finds the rest as you type. Dozens of models, so no list of them all.
  const picked = $derived(cursorModels.filter((m) => cursorShown.includes(m.value)));
  let query = $state("");
  const found = $derived.by(() => {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    if (!words.length) return [];
    return cursorModels
      .filter((m) => !cursorShown.includes(m.value))
      .filter((m) => words.every((w) => `${m.name} ${m.value}`.toLowerCase().includes(w)))
      .slice(0, 8);
  });

  function addModel(value: string) {
    onCursorPick(value, true);
    query = "";
  }

  function findKey(e: KeyboardEvent) {
    if (e.key === "Enter" && found.length) addModel(found[0].value);
    if (e.key === "Escape") query = "";
  }

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

  // Claude Code's limits, by the names it reports them under (CLI 2.1.322):
  // "seven_day" covers all models and "seven_day_overage_included" is Fable's
  // own, matched against `/usage`. Each agent gets its own heading.
  const claudeLimits = $derived(
    [
      { name: "all models", window: "seven_day" },
      { name: "fable", window: "seven_day_overage_included" },
    ].map(({ name, window }) => ({ name, text: limitText(app.limits[CLAUDE]?.[window]) })),
  );

  // Codex's limits, named for how long each window runs ("5h", "week"),
  // as many as its plan has.
  const codexLimits = $derived(
    Object.entries(app.limits[CODEX] ?? {}).map(([name, limit]) => ({ name, text: limitText(limit) })),
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

  /** Escape on a page goes back to the list, not out of Settings. Caught
   *  before the app's own key handler, which closes Settings. */
  function pageKey(e: KeyboardEvent) {
    if (e.key === "Escape" && page) {
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
<div class="settings" data-tauri-drag-region>
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
      <Preferences />
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

    {#if cursorModels.length}
      <section>
        <div class="label">Cursor models</div>
        {#each picked as m (m.value)}
          <div class="row">
            <span class="name">{m.name}</span>
            {#if picked.length > 1}
              <button class="act" onclick={() => onCursorPick(m.value, false)}>remove</button>
            {/if}
          </div>
        {/each}
        <input class="find" placeholder="add a model" spellcheck="false" bind:value={query} onkeydown={findKey} />
        {#each found as m (m.value)}
          <button class="open add" onclick={() => addModel(m.value)}>{m.name}</button>
        {/each}
        {#if query.trim() && !found.length}
          <div class="sub">no match</div>
        {/if}
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
      {#if app.agents.includes(CODEX)}
        <div class="sub">Codex</div>
        {#each codexLimits as limit (limit.name)}
          <div class="row">
            <span class="name">{limit.name}</span>
            <span class="left">{limit.text}</span>
          </div>
        {:else}
          <div class="row"><span class="name">all models</span><span class="left">unknown</span></div>
        {/each}
      {/if}
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

<style>
  .settings {
    position: absolute;
    inset: 0;
    /* Above the side panel's drag edge. */
    z-index: 2;
    background: var(--bg);
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
  .find {
    display: block;
    width: 100%;
    margin-top: calc(var(--gap) * 0.6);
    padding: 0.2em 0;
    font: inherit;
    color: var(--fg);
    background: none;
    border: none;
    border-bottom: 1px solid var(--line);
    outline: none;
  }
  .find::placeholder {
    color: var(--faint);
  }
  .find:focus {
    border-bottom-color: var(--accent);
  }
  .add {
    display: block;
    width: 100%;
  }
  .danger {
    color: var(--bad);
  }
</style>
