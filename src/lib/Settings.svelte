<script lang="ts">
  // Settings, drawn over the chat. A choice applies at once and is remembered.
  import { applyFontSize, applySize, applyTheme, fontSizes, savePrefs, sizes, themes, type Size } from "./themes";

  let {
    theme = $bindable(),
    size = $bindable(),
    fontSize = $bindable(),
    onclose,
  }: { theme: string; size: Size; fontSize: number; onclose: () => void } = $props();

  const groups = [
    { name: "dark", list: themes.filter((t) => t.dark) },
    { name: "light", list: themes.filter((t) => !t.dark) },
  ];

  function pickTheme(id: string) {
    theme = id;
    applyTheme(id);
    savePrefs({ theme: id });
  }

  function pickSize(value: Size) {
    size = value;
    applySize(value);
    savePrefs({ size: value });
  }

  function pickFontSize(px: number) {
    fontSize = px;
    applyFontSize(px);
    savePrefs({ fontSize: px });
  }
</script>

<div class="settings">
  <div class="inner">
    <div class="head">
      <span class="title">Settings</span>
      <button onclick={onclose}>close</button>
    </div>

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
      <div class="label">Theme</div>
      {#each groups as group (group.name)}
        <div class="sub">{group.name}</div>
        <div class="choices grid">
          {#each group.list as t (t.id)}
            <button class:on={t.id === theme} onclick={() => pickTheme(t.id)}>
              <span class="mark">{t.id === theme ? "(•)" : "( )"}</span>{t.name}
            </button>
          {/each}
        </div>
      {/each}
    </section>
  </div>
</div>

<style>
  .settings {
    position: absolute;
    inset: 0;
    background: var(--bg);
    overflow-y: auto;
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
  .head button {
    color: var(--faint);
  }
  .head button:hover {
    color: var(--fg);
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
</style>
