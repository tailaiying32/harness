<script lang="ts">
  // The message box: a prompt marker, a plain text area, and one faint line of
  // context under it. Enter sends; Shift+Enter adds a line.
  import { onMount, tick } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import BranchMark from "../BranchMark.svelte";
  import { agentLabel, clipboardFiles, CLAUDE, CURSOR, onFileDrop, pickFolder, readImageFile, type AgentId, type Mode, type ModelInfo } from "../ipc";
  import { agentModes, type AgentModels } from "../models";
  import { prLabel, prShort, shortPath } from "../paths";
  import { IMAGE_WORD, webLinks, type Chat } from "../state/chat.svelte";
  import { loadPrefs, savePrefs } from "../themes";
  import { emphasis, focusEnd, indent, link, newLine, outdent, pasteLink, renumbered, type Edit } from "./editing";
  import { MAC } from "../state/tabKeys";
  import { imageType } from "./files";
  import { attach, imageFiles } from "./images";
  import CommentDock from "./CommentDock.svelte";
  import { chatInOrder } from "./comments";
  import SuggestMenu from "./SuggestMenu.svelte";
  import { commandsFor, fetchCommands, findCommand, matchCommands, type Command } from "../commands.svelte";

  let {
    chat,
    chats = [],
    ready,
    home,
    models,
    allModels,
    agents,
    focus = 0,
    drops = true,
    onModel,
  }: {
    chat: Chat;
    /** The project's other chats, newest first, to pick one with @. */
    chats?: { id: string; title: string }[];
    ready: boolean;
    home: string;
    /** What the menu offers: for Cursor, the models picked in Settings. */
    models: AgentModels;
    /** Every model, to name a chat's model that isn't offered any more. */
    allModels: AgentModels;
    /** The agents found on this machine, Claude Code first. */
    agents: AgentId[];
    focus?: number;
    /** Whether dropped files land here now; not while something is drawn over the chat. */
    drops?: boolean;
    /** A model was picked, to be remembered for new chats. */
    onModel: (agent: AgentId, model: string) => void;
  } = $props();

  let box: HTMLTextAreaElement;
  let frame: HTMLElement;
  let mirror = $state<HTMLElement>();

  // Opening a chat puts the cursor in its message box, sized to any unsent text.
  onMount(() => {
    grow();
    box.focus();
    // Files dropped anywhere on the window land in the message box.
    return onFileDrop((paths) => {
      if (!drops) return;
      box.focus();
      void addPaths(paths);
    });
  });

  // Asking for a new chat while already on an empty one changes nothing on
  // screen, so the cursor is put in the box by hand.
  $effect(() => {
    if (focus) box?.focus();
  });

  // A queued message taken back to edit lands in the box: fit it, and put
  // the cursor at its end.
  $effect(() => {
    if (!chat.focusDraft) return;
    void tick().then(() => {
      grow();
      focusEnd(box);
    });
  });

  // Each agent has its own modes.
  const modes = $derived(agentModes[chat.agent]);
  const modeLabel = $derived(modes.find((m) => m.id === chat.mode)?.label ?? chat.mode);

  function pickMode(mode: Mode) {
    open = null;
    void chat.changeMode(mode);
    // New chats start in the last mode picked. Plan is for one task, so it
    // isn't kept; Cursor's modes aren't either (Claude Code's are the saved kind).
    if (mode !== "plan" && chat.agent === CLAUDE) savePrefs({ mode });
  }

  const folder = $derived(shortPath(chat.projectPath, home));

  async function changeFolder() {
    const picked = await pickFolder(chat.projectPath || undefined);
    if (picked) chat.projectPath = picked;
  }

  // The lists come from the agents, so they show real names ("Opus 5.5"). A
  // saved choice that is no longer listed still gets an entry, so it shows.
  // An older chat on Claude Code's "default" shows as the model that stands
  // for, which its list puts first; Codex's the same. (Cursor's "default" is Auto, listed.)
  const ownModels = $derived(models[chat.agent] ?? []);
  const shownChoice = $derived(
    chat.agent !== CURSOR && chat.modelChoice === "default" && ownModels.length ? ownModels[0].value : chat.modelChoice,
  );
  const modelOptions = $derived<ModelInfo[]>(
    ownModels.some((m) => m.value === shownChoice)
      ? ownModels
      : [
          allModels[chat.agent]?.find((m) => m.value === shownChoice) ?? { value: shownChoice, name: shownChoice, detail: null },
          ...ownModels,
        ],
  );
  // Every installed agent's models are offered, under its name; picking one
  // puts the chat on that agent. In a chat that has started, the next message
  // hands it over; while a turn runs, other agents wait (greyed out).
  const groups = $derived(
    agents
      .filter((a) => a === chat.agent || (models[a] ?? []).length > 0)
      .map((agent) => ({ agent, list: agent === chat.agent ? modelOptions : models[agent] })),
  );
  const locked = (agent: AgentId) => chat.started && chat.busy && agent !== chat.agent;

  function pickModel(agent: AgentId, choice: string) {
    chat.setAgent(agent, loadPrefs().mode);
    void chat.changeModel(choice);
    onModel(agent, choice);
  }

  // Effort lives in the model menu. A level picked here is for this chat only;
  // new chats start on Claude Code's own default for their model.
  const picked = $derived<ModelInfo | undefined>(modelOptions.find((m) => m.value === shownChoice));
  const efforts = $derived(picked?.efforts ?? []);
  const effort = $derived(
    efforts.includes(chat.effortChoice) ? chat.effortChoice : (picked?.default_effort ?? null),
  );

  // The line under the box has three menus, one open at a time: the chat's
  // pull requests, the model, and the mode.
  type Menu = "prs" | "model" | "mode";
  let open = $state<Menu | null>(null);
  let menus = $state<Record<string, HTMLElement | undefined>>({});
  const toggle = (which: Menu) => (open = open === which ? null : which);

  // Any click outside the open menu, or Escape, closes it.
  $effect(() => {
    const menu = open && menus[open];
    if (!menu) return;
    const away = (e: PointerEvent) => {
      if (!menu.contains(e.target as Node)) open = null;
    };
    const escape = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        open = null;
        box.focus();
      }
    };
    window.addEventListener("pointerdown", away, true);
    window.addEventListener("keydown", escape);
    return () => {
      window.removeEventListener("pointerdown", away, true);
      window.removeEventListener("keydown", escape);
    };
  });

  // The box is measured by letting it collapse for a moment. The composer's
  // own height is held meanwhile: otherwise the chat above would stretch into
  // the freed space and back, and lose its place at the end on every key.
  function grow() {
    frame.style.height = `${frame.offsetHeight}px`;
    box.style.height = "auto";
    box.style.height = `${box.scrollHeight}px`;
    frame.style.height = "";
    syncMirror();
  }

  // ---- Images, files and web addresses ----
  // A pasted or dropped image becomes an `[image n]` word in the text, and a
  // file a `[name]` word. A plain text area can't colour part of its text, so
  // while the draft has any, or a web address, a copy of the text sits behind
  // the box with those coloured, and the box's own text is drawn see-through
  // on top of it.
  //
  // A word opens on a click: the cursor can't go inside it, so a click there
  // has nothing else to do. A web address is text you may still be fixing, so
  // a click puts the cursor in it as usual, and Cmd+click opens it.

  /** The words in the draft that stand for an image or a file, in order. */
  function words(): { at: number; text: string; open: () => void }[] {
    const out: { at: number; text: string; open: () => void }[] = [];
    for (const match of chat.draft.matchAll(IMAGE_WORD)) {
      const image = chat.draftImages.find((i) => i.n === Number(match[1]));
      if (image) out.push({ at: match.index, text: match[0], open: () => chat.openImage(image) });
    }
    for (const file of chat.draftFiles) {
      for (let at = chat.draft.indexOf(file.word); at >= 0; at = chat.draft.indexOf(file.word, at + file.word.length)) {
        out.push({ at, text: file.word, open: () => chat.openDropped(file.path) });
      }
    }
    return out.sort((a, b) => a.at - b.at);
  }

  /** A known command the draft starts with, as `/name`: shown in colour, so a typo stands out by staying plain. */
  const commandWord = $derived.by(() => {
    const typed = chat.draft.match(/^\/([^\s/]+)/);
    return typed && findCommand(commands, typed[1]) ? typed[0] : null;
  });

  /** The draft in pieces: plain text, a command, the words of images and files it carries, and web addresses. */
  const pieces = $derived.by(() => {
    const found: { at: number; text: string; open?: () => void; url?: string; command?: boolean }[] = [
      ...(commandWord ? [{ at: 0, text: commandWord, command: true }] : []),
      ...words(),
      ...webLinks(chat.draft).map((link) => ({ ...link, url: link.text })),
    ].sort((a, b) => a.at - b.at);
    if (!found.length) return null;
    const out: { text: string; open?: () => void; url?: string; command?: boolean }[] = [];
    let at = 0;
    for (const piece of found) {
      if (piece.at < at) continue;
      out.push(
        { text: chat.draft.slice(at, piece.at) },
        { text: piece.text, open: piece.open, url: piece.url, command: piece.command },
      );
      at = piece.at + piece.text.length;
    }
    // A final newline needs something after it to take up its line.
    out.push({ text: chat.draft.slice(at) + "\u200b" });
    return out;
  });

  /** The word or web address under the pointer, found where the copy behind the box draws it. */
  function pieceAt(x: number, y: number) {
    if (!mirror || !pieces) return undefined;
    for (const span of mirror.querySelectorAll<HTMLElement>("[data-piece]")) {
      for (const r of span.getClientRects()) {
        if (x >= r.left && x < r.right && y >= r.top && y < r.bottom) return pieces[Number(span.dataset.piece)];
      }
    }
  }

  // The pointer turns to a hand over a word, and over a web address while Cmd
  // is held. Only looked at while the pointer is over the box.
  let pointer: { x: number; y: number } | null = null;

  function hover(cmd: boolean) {
    const piece = pointer ? pieceAt(pointer.x, pointer.y) : undefined;
    box.style.cursor = piece?.open || (piece?.url && cmd) ? "pointer" : "";
  }

  function onPointerMove(e: PointerEvent) {
    pointer = { x: e.clientX, y: e.clientY };
    hover(e.metaKey);
  }

  function onPointerLeave() {
    pointer = null;
    box.style.cursor = "";
  }

  // Pressing or letting go of Cmd with the pointer still.
  $effect(() => {
    const key = (e: KeyboardEvent) => {
      if (pointer && e.key === "Meta") hover(e.metaKey);
    };
    window.addEventListener("keydown", key);
    window.addEventListener("keyup", key);
    return () => {
      window.removeEventListener("keydown", key);
      window.removeEventListener("keyup", key);
    };
  });

  /** Keeps the copy behind the box lined up: same width (less any scroll bar) and scroll. */
  function syncMirror() {
    if (!mirror) return;
    mirror.style.width = `${box.clientWidth}px`;
    mirror.scrollTop = box.scrollTop;
  }

  $effect(() => {
    if (pieces) void tick().then(syncMirror);
  });

  /** Puts a word where the cursor is, as typing would, so undo works. */
  async function insertWord(word: string) {
    const { selectionStart: from, selectionEnd: to } = box;
    const before = chat.draft.slice(0, from);
    const insert = (before && !/\s$/.test(before) ? " " : "") + word;
    await apply({ from, to, insert, selStart: from + insert.length, selEnd: from + insert.length });
  }

  /** Keeps the images and puts their words where the cursor is. */
  async function addImages(files: Blob[]) {
    for (const file of files) {
      try {
        await insertWord(await attach(chat, file));
      } catch (e) {
        chat.showError(e instanceof Error ? e.message : String(e));
      }
    }
  }

  /**
   * Takes dropped or pasted files by path. An image becomes `[image n]`, as a
   * pasted one does; anything else, or an image that can't be read, becomes
   * a file word, and Claude opens the file itself.
   */
  async function addPaths(paths: string[]) {
    for (const path of paths) {
      const type = imageType(path);
      if (type) {
        try {
          const bytes = await readImageFile(path);
          await insertWord(await attach(chat, new Blob([bytes], { type })));
          continue;
        } catch {
          // Not an image the app can take in: it goes as a file.
        }
      }
      await insertWord(await chat.addDraftFile(path));
    }
  }

  // A paste with files in it may be files copied in Finder; the page is given
  // their names but not where they are, so the core is asked. Otherwise only
  // a paste with no text in it becomes an image: copied spreadsheet cells
  // carry a picture of themselves too, and should paste as text.
  function onPaste(e: ClipboardEvent) {
    const data = e.clipboardData;
    if (!data) return;
    // A web address pasted over selected text makes it a link.
    const asLink = pasteLink(chat.draft, box.selectionStart, box.selectionEnd, data.getData("text/plain"));
    if (asLink) {
      e.preventDefault();
      void apply(asLink, false);
      return;
    }
    const items = [...data.items].filter((i) => i.kind === "file");
    if (!items.length && !data.types.includes("Files")) return;
    const text = data.getData("text/plain");
    const images = imageFiles(items.map((i) => i.getAsFile()));
    e.preventDefault();
    void (async () => {
      const paths = await clipboardFiles().catch(() => []);
      if (paths.length) await addPaths(paths);
      else if (text) {
        const { selectionStart: from, selectionEnd: to } = box;
        await apply({ from, to, insert: text, selStart: from + text.length, selEnd: from + text.length });
      } else await addImages(images);
    })();
  }

  // Clicking an image's or a file's word opens it; Cmd+click a web address
  // opens it in the default browser. A drag that selected text opens nothing.
  function onClick(e: MouseEvent) {
    if (box.selectionStart !== box.selectionEnd) return;
    const piece = pieceAt(e.clientX, e.clientY);
    if (piece?.open) piece.open();
    else if (piece?.url && e.metaKey) void openUrl(piece.url).catch(() => {});
  }

  // A word is one thing: the cursor steps over it, never into it, and a
  // selection takes all of it or none.

  /** Where the moving end of the selection last came to rest. */
  let lastFocus = 0;
  /** While the mouse is down the cursor is left alone; it is moved out of any word once the click is done. */
  let pressing = false;

  /**
   * Moves a selection end that sits inside a word to one of the word's edges:
   * onward in the direction it was moving, or to the nearer edge when
   * `nearest` (after a click).
   */
  function snap(nearest = false) {
    if (!box || document.activeElement !== box) return;
    const { selectionStart: start, selectionEnd: end } = box;
    const backward = box.selectionDirection === "backward";
    const focus = backward ? start : end;
    const anchor = backward ? end : start;
    const list = start === lastFocus && end === lastFocus ? [] : words();
    const around = (at: number) => list.find((w) => at > w.at && at < w.at + w.text.length);
    let to = focus;
    const word = around(focus);
    if (word) {
      const [from, till] = [word.at, word.at + word.text.length];
      const onward = nearest || focus === lastFocus ? focus - from >= till - focus : focus > lastFocus;
      to = onward ? till : from;
    }
    let base = start === end ? to : anchor;
    const held = start === end ? undefined : around(anchor);
    if (held) base = anchor <= to ? held.at : held.at + held.text.length;
    lastFocus = to;
    if (to !== focus || base !== anchor) {
      box.setSelectionRange(Math.min(base, to), Math.max(base, to), to < base ? "backward" : "forward");
    }
  }

  $effect(() => {
    const moved = () => {
      if (!pressing) snap();
      findSuggest();
    };
    const released = () => {
      if (!pressing) return;
      pressing = false;
      // After the click itself, which may open the word under the pointer.
      setTimeout(() => snap(true));
    };
    document.addEventListener("selectionchange", moved);
    window.addEventListener("pointerup", released);
    return () => {
      document.removeEventListener("selectionchange", moved);
      window.removeEventListener("pointerup", released);
    };
  });

  // ---- Picking a chat with @, or a command with / ----
  // Typing @ at the start of a word lists the project's 8 newest other chats
  // that match what is typed after it. A picked chat becomes a word,
  // `[@Title]`, kept like a dropped file's: it is one thing in the box, a
  // click opens the chat, and the agent is sent the chat's ID to read it by.
  //
  // Typing / at the very start of the message lists the commands this chat's
  // agent takes here, with Harness's skills and `/model`. A picked command is
  // typed in as plain text. A command not in the list is still sent as typed.

  /** The @ or / being typed: where it and the cursor are, and the row the keys are on. */
  let suggest = $state<{ kind: "chat" | "command"; at: number; end: number; row: number } | null>(null);
  /** An @ or / whose list was closed with Escape: it stays closed. */
  let dismissed = -1;

  // The commands are read from the agent the first time a chat on it shows
  // in a folder (once per run), once the agents are found, and again on a
  // `/` if that failed; until then the last list is offered.
  $effect(() => {
    if (ready) fetchCommands(chat.agent, chat.root);
  });
  const commands = $derived(commandsFor(chat.agent, chat.root));

  const typedWord = $derived(suggest ? chat.draft.slice(suggest.at + 1, suggest.end) : "");
  const mentioned = $derived.by(() => {
    if (suggest?.kind !== "chat") return [];
    const typed = typedWord.toLowerCase();
    return chats.filter((c) => c.title.toLowerCase().includes(typed)).slice(0, 8);
  });
  const commanded = $derived<Command[]>(suggest?.kind === "command" ? matchCommands(commands, typedWord) : []);
  const suggestRows = $derived(
    suggest?.kind === "chat"
      ? mentioned.map((c) => ({ key: c.id, label: c.title }))
      : commanded.map((c) => ({ key: c.name, label: `/${c.name}`, detail: c.description })),
  );

  /** Looks for an @ or a / being typed just before the cursor. */
  function findSuggest() {
    const at = box && document.activeElement === box && box.selectionStart === box.selectionEnd ? box.selectionStart : -1;
    const before = at < 0 ? "" : chat.draft.slice(0, at);
    // A / only as the message's first word; an @ after a space or at the
    // start, so an address like me@example.com isn't one.
    const command = before.match(/^\/([^\s/]*)$/);
    const mention = command || !chats.length ? null : before.match(/(?:^|\s)@((?:\S[^\n@[\]]{0,39})?)$/);
    const typed = command ?? mention;
    if (!typed) {
      suggest = null;
      dismissed = -1;
      return;
    }
    const kind = command ? "command" : "chat";
    if (command && ready) fetchCommands(chat.agent, chat.root);
    const start = at - typed[1].length - 1;
    if (start === dismissed) suggest = null;
    else if (suggest?.at !== start || suggest.end !== at || suggest.kind !== kind) suggest = { kind, at: start, end: at, row: 0 };
  }

  /** Puts the picked row in where the @ or / and what was typed after it are. */
  async function pick(i: number) {
    if (!suggest) return;
    const { kind, at, end } = suggest;
    // Read before the list closes: the lists follow `suggest`.
    const chatPicked = mentioned[i];
    const commandPicked = commanded[i];
    suggest = null;
    if (kind === "chat") {
      const picked = chatPicked;
      if (!picked) return;
      const insert = chat.addDraftChat(picked.id, picked.title) + " ";
      await apply({ from: at, to: end, insert, selStart: at + insert.length, selEnd: at + insert.length });
      return;
    }
    const picked = commandPicked;
    if (!picked) return;
    if (picked.harness === "model") {
      // Take out what was typed (up to the end of the word) and open the menu.
      const word = chat.draft.slice(at).match(/^\S*\s?/)?.[0] ?? "";
      await apply({ from: at, to: at + word.length, insert: "", selStart: at, selEnd: at });
      open = "model";
      return;
    }
    // The whole first word is replaced, if the cursor was inside it.
    const word = chat.draft.slice(at).match(/^\S*/)?.[0] ?? "";
    const insert = `/${picked.name} `;
    const to = at + word.length + (chat.draft[at + word.length] === " " ? 1 : 0);
    await apply({ from: at, to, insert, selStart: at + insert.length, selEnd: at + insert.length });
  }

  /** The keys the open list takes: up and down, Enter or Tab to pick, Escape to close. */
  function suggestKey(e: KeyboardEvent): boolean {
    if (!suggest || !suggestRows.length) return false;
    const last = suggestRows.length - 1;
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      // The best is at the bottom, so Up goes further down the list.
      const row = Math.min(suggest.row, last) + (e.key === "ArrowUp" ? 1 : -1);
      suggest.row = Math.max(0, Math.min(last, row));
    } else if ((e.key === "Enter" || e.key === "Tab") && !e.shiftKey) {
      void pick(Math.min(suggest.row, last));
    } else if (e.key === "Escape") {
      dismissed = suggest.at;
      suggest = null;
      e.stopPropagation();
    } else {
      return false;
    }
    e.preventDefault();
    return true;
  }

  async function send() {
    const text = chat.draft;
    // `/model` is Harness's own: it opens the model menu, and nothing is sent.
    if (/^\/model(\s|$)/.test(text.trim())) {
      chat.draft = "";
      await tick();
      grow();
      open = "model";
      return;
    }
    // Comments on the latest answer go with the message, or alone. A command
    // goes as typed and leaves them waiting.
    const command = text.trim().startsWith("/");
    const comments = !command && chat.chatComments.length > 0;
    if ((!text.trim() && !comments) || !ready) return;
    chat.draft = "";
    await tick();
    grow();
    void (command ? chat.send(text) : chat.sendWithComments(text));
  }

  /** Makes one replacement in the box the way typing would, so undo still works. */
  async function apply(asked: Edit, renumber = true) {
    // Numbered lists the edit disturbs are put back in order in the same step.
    const edit = renumber ? renumbered(chat.draft, asked) : asked;
    box.setSelectionRange(edit.from, edit.to);
    const typed = edit.insert
      ? document.execCommand("insertText", false, edit.insert)
      : edit.from === edit.to || document.execCommand("delete");
    if (!typed) {
      chat.draft = chat.draft.slice(0, edit.from) + edit.insert + chat.draft.slice(edit.to);
      await tick();
    }
    box.setSelectionRange(edit.selStart, edit.selEnd);
    grow();
  }

  // The box is for writing, so its keys work as in a document: Tab indents
  // and Shift+Tab pulls back (Tab never leaves the box), and a new line in a
  // list starts the next item. Enter alone still sends.
  function onKey(e: KeyboardEvent) {
    if (e.isComposing) return;
    if (suggestKey(e)) return;
    const { selectionStart: start, selectionEnd: end } = box;
    // Backspace just after a word, or Delete just before one, removes all of it.
    if ((e.key === "Backspace" || e.key === "Delete") && start === end && !e.metaKey && !e.ctrlKey && !e.altKey) {
      const word = words().find((w) => (e.key === "Backspace" ? w.at + w.text.length : w.at) === start);
      if (word) {
        e.preventDefault();
        void apply({ from: word.at, to: word.at + word.text.length, insert: "", selStart: word.at, selEnd: word.at });
        return;
      }
    }
    // Cmd+B bold, Cmd+I italic, Cmd+K link, on or off, as in the docs editor.
    const format = (MAC ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase();
    if (format === "b" || format === "i" || format === "k") {
      e.preventDefault();
      void apply(format === "k" ? link(chat.draft, start, end) : emphasis(chat.draft, start, end, format === "b" ? 2 : 1), false);
      return;
    }
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void send();
    } else if (e.key === "Enter") {
      const edit = newLine(chat.draft, start, end);
      if (!edit) return;
      e.preventDefault();
      void apply(edit);
    } else if (e.key === "Tab" && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault();
      const edit = e.shiftKey ? outdent(chat.draft, start, end) : indent(chat.draft, start, end);
      if (edit) void apply(edit);
    }
  }
</script>

<!-- data-doc-drop: a doc dragged from the docs menu lands here like a dropped file. -->
<div class="composer" data-doc-drop bind:this={frame}>
  <div class="inner">
    <!-- Comments on the latest answer, waiting to go with the next message. -->
    {#if !chat.owner && chat.chatComments.length}
      <CommentDock
        items={chatInOrder(chat.chatComments)}
        onjump={(id) => chat.jumpToChatComment?.(id)}
        ondrop={(id) => chat.removeChatComment(id)}
      />
    {/if}
    <div class="prompt">
      <span class="marker">&gt;</span>
      <div class="field">
        {#if pieces}
          <div class="mirror" bind:this={mirror} aria-hidden="true">
            {#each pieces as piece, i (i)}{#if piece.open}<span class="word" data-piece={i}>{piece.text}</span>{:else if piece.command}<span class="command">{piece.text}</span>{:else if piece.url}<span class="link" data-piece={i}>{piece.text}</span>{:else}{piece.text}{/if}{/each}
          </div>
        {/if}
        <textarea
          bind:this={box}
          bind:value={chat.draft}
          class:see-through={!!pieces}
          oninput={() => {
            grow();
            findSuggest();
          }}
          onblur={() => (suggest = null)}
          onkeydown={onKey}
          onpaste={onPaste}
          onclick={onClick}
          onpointermove={onPointerMove}
          onpointerleave={onPointerLeave}
          onpointerdown={() => (pressing = true)}
          onkeyup={() => snap()}
          onscroll={syncMirror}
          rows="1"
          placeholder={`Message ${agentLabel(chat.agent)}`}
        ></textarea>
        {#if suggest && suggestRows.length}
          <SuggestMenu
            rows={suggestRows}
            bind:row={suggest.row}
            onPick={(i) => void pick(i)}
            label={suggest.kind === "chat" ? "Chats in this project" : "Commands"}
            wide={suggest.kind === "command"}
          />
        {/if}
      </div>
      {#if chat.busy}
        <button class="stop" onclick={() => chat.stop()}>stop</button>
      {/if}
    </div>

    <div class="meta">
      <!-- Laid out like the Claude desktop app: on a new chat the folder picker
           sits at the left; once the chat has started it is gone (the sidebar
           shows the folder). Model and mode sit at the right. -->
      {#if !chat.started}
        <button class="folder" onclick={changeFolder} title="Choose the folder this chat works in">
          {folder}
        </button>
      {:else if chat.prs.length}
        <!-- The chat's pull requests: one mark and a count; the menu lists
             them. A worktree with no pull request yet shows nothing. -->
        <span class="picker" bind:this={menus.prs}>
          <button
            class="prs"
            onclick={() => toggle("prs")}
            title={prLabel(chat.prs)}
            aria-haspopup="menu"
            aria-expanded={open === "prs"}
          >
            <BranchMark prs={chat.prs} />
            {prShort(chat.prs)}
          </button>
          {#if open === "prs"}
            <div class="menu left" role="menu">
              {#each chat.prs as pr (pr.url)}
                <button role="menuitem" class="row" title="Open on GitHub" onclick={() => openUrl(pr.url)}>
                  <span class="pr {pr.state}">#{pr.number} {pr.state}</span>
                  <span>{pr.branch}</span>
                </button>
              {/each}
              {#if chat.workPath}
                <div class="where" title="This chat works in its own worktree">{shortPath(chat.workPath, home, 60)}</div>
              {/if}
            </div>
          {/if}
        </span>
      {/if}
      <span class="spacer"></span>
      <span class="picker model" bind:this={menus.model}>
        <button onclick={() => toggle("model")} title="Model and effort" aria-haspopup="menu" aria-expanded={open === "model"}>
          {picked?.name ?? chat.modelChoice}{#if effort}&ensp;{effort}{/if}
        </button>
        {#if open === "model"}
          <div class="menu" role="menu">
            {#each groups as group (group.agent)}
              {#if groups.length > 1}
                <div class="group">{agentLabel(group.agent)}</div>
              {/if}
              {#each group.list as m (m.value)}
                {@const on = group.agent === chat.agent && m.value === shownChoice}
                <button
                  role="menuitemradio"
                  aria-checked={on}
                  class:on
                  title={m.detail ?? ""}
                  disabled={locked(group.agent)}
                  onclick={() => pickModel(group.agent, m.value)}>{m.name}</button
                >
              {/each}
            {/each}
            {#if efforts.length}
              <div class="efforts" role="group" aria-label="Effort">
                <span>effort</span>
                {#each efforts as level (level)}
                  <button
                    role="menuitemradio"
                    aria-checked={level === effort}
                    class:on={level === effort}
                    onclick={() => chat.changeEffort(level)}>{level}</button
                  >
                {/each}
              </div>
            {/if}
          </div>
        {/if}
      </span>
      <span class="picker" bind:this={menus.mode}>
        <button onclick={() => toggle("mode")} title="Mode" aria-haspopup="menu" aria-expanded={open === "mode"}>
          {modeLabel}
        </button>
        {#if open === "mode"}
          <div class="menu" role="menu">
            {#each modes as m (m.id)}
              <button
                role="menuitemradio"
                aria-checked={m.id === chat.mode}
                class:on={m.id === chat.mode}
                onclick={() => pickMode(m.id)}>{m.label}</button
              >
            {/each}
          </div>
        {/if}
      </span>
    </div>
  </div>
</div>

<style>
  .composer {
    padding: 0 var(--pad-x) var(--foot-y);
  }
  .inner {
    max-width: var(--col);
    margin: 0 auto;
    border-top: 1px solid var(--line);
    padding-top: var(--gap);
  }
  .prompt {
    display: flex;
  }
  .marker {
    width: 2.5ch;
    flex-shrink: 0;
    color: var(--accent);
  }
  .field {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
  }
  /* Behind the box, drawn exactly as the box draws its text. */
  .mirror {
    position: absolute;
    top: 0;
    left: 0;
    bottom: 0;
    overflow: hidden;
    white-space: pre-wrap;
    overflow-wrap: break-word;
    color: var(--bright);
    pointer-events: none;
  }
  .mirror .word,
  .mirror .command {
    color: var(--accent);
  }
  .mirror .link {
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  textarea.see-through {
    position: relative;
    color: transparent;
  }
  textarea {
    flex: 1;
    min-width: 0;
    resize: none;
    border: 0;
    outline: 0;
    padding: 0;
    background: transparent;
    color: var(--bright);
    line-height: inherit;
    max-height: 40vh;
    caret-color: var(--accent);
  }
  textarea::placeholder {
    color: var(--faint);
  }
  .stop {
    align-self: flex-start;
    color: var(--bad);
    margin-left: 2ch;
  }
  /* One line always: the folder gives way first when the window is narrow. */
  .meta {
    display: flex;
    align-items: center;
    gap: 3ch;
    white-space: nowrap;
    margin-top: var(--foot-gap);
    color: var(--faint);
  }
  .meta > * {
    flex-shrink: 0;
  }
  .meta .folder {
    flex-shrink: 1;
    min-width: 6ch;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta .prs {
    display: flex;
    align-items: center;
    gap: 1ch;
  }
  .pr.open {
    color: var(--ok);
  }
  .pr.draft {
    color: var(--dim);
  }
  .pr.merged {
    color: var(--purple);
  }
  .pr.closed {
    color: var(--bad);
  }
  /* In a narrow chat the model's label gives way with an ellipsis, so the
     row never runs past the chat's edge. The menu hangs off the span, so the
     clipping is on the button only. */
  .meta .model {
    flex-shrink: 1;
    min-width: 6ch;
  }
  .meta .model > button {
    display: block;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta .spacer {
    flex: 1 1 0;
    min-width: 0;
  }
  .meta button:not(:disabled):hover {
    color: var(--fg);
  }
  /* A menu opens upward from its label: the model's right-aligned with it,
     the pull requests' left-aligned. A model click picks and leaves the menu
     open, so effort can be set in the same visit. */
  .picker {
    position: relative;
  }
  .menu {
    position: absolute;
    right: 0;
    bottom: calc(100% + var(--gap) * 0.4);
    z-index: 10;
    display: flex;
    flex-direction: column;
    max-height: 60vh;
    overflow-y: auto;
    padding: calc(var(--gap) * 0.4) 0;
    background: var(--panel);
    border: 1px solid var(--line);
  }
  .menu button {
    text-align: left;
    padding: 0 2ch;
    color: var(--dim);
  }
  .menu button.on {
    color: var(--accent);
  }
  /* Another agent's models while a turn runs: picked again once it ends. */
  .menu button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .menu.left {
    right: auto;
    left: 0;
  }
  .menu .row {
    display: flex;
    gap: 2ch;
  }
  /* Wide enough for "#123 merged", so the branches line up. */
  .menu .row .pr {
    min-width: 11ch;
  }
  .menu .row:hover {
    color: var(--fg);
  }
  .where {
    margin-top: calc(var(--gap) * 0.4);
    padding: calc(var(--gap) * 0.4) 2ch 0;
    border-top: 1px solid var(--line);
    color: var(--faint);
  }
  /* An agent's name over its models; a line above every one but the first. */
  .group {
    padding: 0 2ch;
    color: var(--faint);
  }
  .group ~ .group {
    margin-top: calc(var(--gap) * 0.4);
    padding-top: calc(var(--gap) * 0.4);
    border-top: 1px solid var(--line);
  }
  .efforts {
    display: flex;
    gap: 1.5ch;
    margin-top: calc(var(--gap) * 0.4);
    padding: calc(var(--gap) * 0.4) 2ch 0;
    border-top: 1px solid var(--line);
    color: var(--faint);
  }
  .efforts button {
    padding: 0;
  }
</style>
