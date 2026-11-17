# Harness to-do

One list for the whole project. Add things "for later" here; tick or remove
them when done. Details live in PLAN.md.

## To decide

- [ ] **Who pays for generated titles.** Chat titles and subagent batch labels
  run a small Haiku call on the user's Claude login. Without Claude Code, or
  signed out, they fall back to text cut from the message. Open:
  - API-key users get none (the app strips the key from the call). Allow it,
    knowing each title is a small charge?
  - Cursor chats never get batch labels. Worth a cheap Cursor call?
  - A setting to turn generated titles off?

## To build

- [ ] Token count on finished subagent rows ("12 steps · 34k tokens"). The core drops Claude Code's total; foreground subagents may only report it in
  the Agent call's result.
- [ ] Pick a file by key or command, `@` file search, fold long pasted text.
- [ ] Settings commands (the store already has `setting` / `set_setting`).
- [ ] Rename a chat from the top bar.

## To try in the real app

- [ ] Subagent batch labels from Haiku; step counts on finished subagents.
- [ ] "Pull up the to-do list" opening it in the panel (the `harness_panel`
  show tool), and "show me the terminal".
- [ ] Effort picker.
- [ ] Editing a queued message from the message box.
- [ ] Links: a markdown file, a web link, another site from the browser pane.
- [ ] Picking a different folder in the native folder picker.
- [ ] A real pull request going through its states.
- [ ] Browser pane by eye: lines up over the panel, follows its drag, hides
  under Settings and the image view.
- [ ] Dragging panel tabs, by eye.
- [ ] The docs button; Cursor's doc tab read from disk.
- [ ] Plans: "keep planning" with feedback.
- [ ] Cursor: the sign-in card, a question card, "keep planning" from the panel.
- [ ] A chat long enough to load in parts; a chat whose context was squeezed.
- [ ] Windows: build, top bar, browser pane.

## To measure

- [ ] Repeat the memory comparison with the desktop app (styled text, a
  day-long chat, window in front).
