# Harness can own recall across every agent

Nobody ships what Harness is aiming for: a local, built-in way for any agent to find and read past chats and share one memory. Vendors keep both inside their own walls. Claude Code's memory is invisible to other agents. Cursor dropped its memory feature. The only cross-agent memory found (Warp) is cloud-only and waitlisted. For finding chats, the evidence favours plain word search with the agent retrying wordings over search by meaning, though that evidence is thinner than it looks. For reading chats, the clearest lesson comes from Amp: don't hand over a whole chat, and check later turns, because early answers often got reverted. For memory, every tool that writes memories on its own ends up adding a way to review them, dates, or expiry. Cursor's attempt failed in public. Harness already stores every chat from every agent in one database, so it can build both pieces locally with no new dependencies, then add meaning search and summaries only if misses show up.

## What we researched

Four research passes (November 2026), mostly from vendor docs and changelogs, with forum threads and third-party write-ups flagged where used:
- **Vendor coding agents:** Claude Code, Claude's app, Codex, Cursor, Windsurf/Devin, GitHub Copilot.
- **Agent development environments and open agents:** Amp, Zed, Warp, Conductor, Cline, Kiro, Jules, Augment.
- **Standalone memory layers:** Mem0, Letta, Zep, Hindsight, basic-memory, Memorix, Anthropic's memory tool, plus the benchmark evidence.
- **Light on-device search** for a Rust + SQLite app: SQLite's built-in word search, sqlite-vec, small embedding models, OS-provided options.

Not checked: OpenCode, Roo Code, and the worktree tools (Crystal, Claude Squad, Sculptor, Vibe Kanban). We ran no benchmarks ourselves.

## Key findings

**Finding past chats is weak almost everywhere**
- Claude's app is the clearest case. It has a search tool and a recent-chats tool. It returns matching pieces with links back to the source chats, and stays inside the current project ([Claude Help Center](https://support.claude.com/en/articles/11817273-use-claude-s-chat-search-and-memory-to-build-on-previous-context)). Whether it matches words or meaning is not stated.
- Claude Code has no documented tool for searching past chats. One user report says the desktop app's session search is raw substring match, so swapping two words returns nothing ([issue, unverified](https://claudeissues.com/issue/88516-session-history-search-is-a-raw-substring-match-reordering-two-words-returns-zer)).
- Cursor added transcript search for users in July 2026 ([changelog](https://cursor.com/changelog/side-chat)). Agent-side past-chat search is reported but not confirmed.
- Community tools fill the gap, and some search across Claude Code, Codex and Cursor at once ([omnirecall roundup](https://scottspence.com/posts/omnirecall-search-claude-code-codex-and-pi-sessions)).

**Amp learned the hard way how to read another chat**
- Amp splits it in two: a find tool (keyword, file touched, date, author) and a read tool ([Amp: Find Threads](https://ampcode.com/news/find-threads)).
- Its first reader sent the whole chat to one model call. That failed on long chats, even at 1M tokens. It also answered from the first plausible hit, which had often been reverted later.
- The rewrite is a helper agent that takes a question, searches the chat, and checks for later turns that "revise, supersede, revert, or contradict" the hit. It treats tool calls as attempts, not proof they worked ([Amp: Read Bigger Threads](https://ampcode.com/news/read-bigger-threads)).

**Memory is split by vendor, and only Warp crosses agents**
- Claude Code writes per-repo markdown notes and loads a short index at start ([Claude Code docs](https://code.claude.com/docs/en/memory)). No other agent sees them.
- Warp shares one memory across its own agent, Claude Code and Codex. But it's a waitlisted preview, third-party agents work only in Warp's cloud, and it doesn't say whether Claude Code's own memory is turned off ([Warp docs](https://docs.warp.dev/agents/agent-memory/)).
- No product says how it handles an agent writing its own memory alongside a shared one.

**Memory the user can't see goes bad**
- Cursor removed its Memories feature in 2.1 and steered users to rule files ([forum](https://forum.cursor.com/t/are-my-memories-gone/144057)). Then came complaints about "ghost" rules and memories still being used after deletion ([forum](https://forum.cursor.com/t/ghost-user-rules-still-injected-in-agent-context-empty-in-settings-user-tab-not-in-aicontext-personalcontext-logout-login-did-not-fix/164709)).
- Every tool that keeps auto-written memory adds a check of some kind: review screens (Augment, Warp, Claude's app), dates (Claude Code), or re-checks and expiry (Copilot).

**Keyword search plus a retrying agent holds up, but the evidence is narrow**
- "Is Grep All You Need?" tested 116 long chat-history questions. Plain grep generally beat vector search, but results "depend strongly" on the agent harness ([arXiv](https://arxiv.org/abs/2605.15184)).
- In one setup grep scored 93.1% against 83.6% for vectors. The same model inside Claude Code scored 76.7%, and Gemini CLI did better with vectors ([secondhand summary](https://blakecrosley.com/blog/agent-search-is-a-runtime-problem)).
- The authors warn the result may not hold for less literal content. Loose pointing ("that chat about the sidebar thing") is exactly that.

## How each tool does it

| Tool | Find past chats | Read another chat | Memory: who writes it | Memory: scope, where | User can see/edit |
|---|---|---|---|---|---|
| **Claude app** | Search tool + recent-chats tool, project-scoped | Matching pieces with links | Claude, as topics during chat | Account + per project, cloud | Yes, per topic |
| **Claude Code** | None documented | Resume only | Agent, during chat | Per repo, local markdown | Yes, plain files |
| **Codex** | No cross-chat search | Mention a chat to pull its messages in (third-party report) | Background job after idle (off by default) | Local files | Files, "not for hand-editing" |
| **Cursor** | User transcript search (3.11); agent search reported | Mention past chats (reported) | Removed in 2.1; rule files instead | Rules in repo | Rules yes; old memories hard to clear |
| **Amp** | Find tool: keyword, file, date, author | Question-focused helper agent | No auto memory; AGENTS.md | Cloud chats, workspace sharing | n/a |
| **Zed** | History list, mention a past chat | "New from summary" | None built in | n/a | n/a |
| **Warp** (preview) | Not described | Not described | Extracted after each chat; merges or replaces | Cloud stores: personal, agent, team | Web app, source + full edit history |
| **Copilot** | None found | None found | Agent, during work | Cloud: repo facts + user prefs | View/delete, not edit |
| **Windsurf / Devin** | Not verified | Not verified | Agent, silent (Windsurf); suggested + approved (Devin) | Per workspace, local (Windsurf) | Yes |
| **Cline** | None | None | User/agent maintain "memory bank" files | Repo files | Yes, plain files |

**Notes worth knowing**
- **Claude Code** loads only the first 200 lines or 25 KB of its index. Other notes are read on demand. When the index grows, Claude is told to merge or drop stale entries. It stamps a date on every write and shows "Saved N memories" in the chat. It skips anything the code can already tell you. It can be turned off with a setting or an environment variable, and its folder can be moved ([docs](https://code.claude.com/docs/en/memory)).
- **Codex** waits until a chat has been idle, extracts memories, then merges them across chats under a lock ([OpenAI docs](https://learn.chatgpt.com/docs/customization/memories?surface=app); merge details [third-party](https://codex.danielvaughan.com/2026/04/18/codex-built-in-memory-system-deep-dive/)). It skips a run when your usage limit is low. It can also skip chats that used tools or web search, which is a guard against poisoning.
- **Copilot** links every memory to the code or the user words behind it, and re-checks that link before use. Memories expire after 90 days unless confirmed again; just being retrieved doesn't count ([GitHub Docs](https://docs.github.com/en/copilot/concepts/agents/copilot-memory)).
- **Claude's app** keeps memories after you delete the chat they came from ([Help Center](https://support.claude.com/en/articles/11817273-use-claude-s-chat-search-and-memory-to-build-on-previous-context)).
- **Amp** also finds "every chat that touched this file." It tags commits with the chat they came from, and pressing Enter twice in a new chat references the one you just left ([Amp docs](https://ampcode.com/docs/threads)).

## The methods and their tradeoffs

### Finding a chat

| Method | Catches different wording? | Cost | Who uses it |
|---|---|---|---|
| Title only | No | None | Claude Code VS Code, early on ([issue](https://claudeissues.com/issue/50344-vs-code-sidebar-search-sessions-is-title-only-add-full-text-search-over-session)) |
| Exact substring | No; word order breaks it | None | Claude Code desktop (reported) |
| Ranked word index with stemming | Word forms only ("ran" finds "running"), not synonyms | None for Harness: built into the SQLite it already ships ([build script](https://github.com/rusqlite/rusqlite/blob/master/libsqlite3-sys/build.rs)); index is 8–45% of text size ([SQLite](https://www.sqlite.org/fts5.html)) | Amp (likely), memory tools |
| Same, with agent retrying wordings | Yes, the agent supplies the synonyms | A few extra tool calls | Claude Code style ([arXiv](https://arxiv.org/abs/2605.15184)) |
| Meaning search (embeddings) | Yes, but pulls in loosely related hits | A model: 8–30 MB static ([model2vec](https://github.com/MinishLab/model2vec)) up to ~200 MB loaded for better ones ([HF](https://huggingface.co/blog/embeddinggemma)) | Cursor forum asks for it ([forum](https://forum.cursor.com/t/implement-semantic-search-for-past-chats-as-well/143060)) |
| Both, merged by rank | Best of both, in theory | Both costs | basic-memory, Zep |
| By file, date, recent list, @-mention | Doesn't need words at all | Cheap | Amp, Claude app, Zed, Codex |

**Retrying works because the agent does the synonym work**
- Letta beat Mem0's reported score with a plain file and simple search tools: 74.0% vs 68.5%, both vendor-reported ([Letta](https://www.letta.com/blog/benchmarking-ai-agent-memory)). But its search tool also did meaning search, so this wasn't pure keyword.
- The usual benchmark chats are only 16–26K tokens, small enough to fit in context, which makes them weak tests.
- No independent study settles keyword vs meaning search for fuzzy "what did we decide about X" questions over chat history.

**The tiny model is cheap but weaker**
- The 30 MB static model scores 35.1 on retrieval against 42.9 for the common small model, MiniLM, about 18% worse ([model2vec results](https://github.com/MinishLab/model2vec/blob/main/results/README.md)).
- At one vector per chat (hundreds of chats), search cost is negligible and doesn't even need a vector extension.
- The OS-provided options (Apple's built-in embeddings, Windows' content search API) differ across platforms, are unbenchmarked or experimental, and would rank differently on Mac and Windows.

### Reading a chat

**Every way of reading trades detail against context or usage**
- **Whole chat in:** simple, but fails on long chats (Amp) and floods the caller's context.
- **Generic summary** (Zed, Amp handoff): cheap and quick, but can't answer a specific question and loses exact wording.
- **Matching pieces with links** (Claude app): small and precise, but can miss that a later turn undid the piece.
- **Question-focused helper** (Amp): best answers, and checks later turns. Costs a separate model call and adds wait time.
- **Caller pages through it:** no extra model, but uses the caller's context and relies on the caller to check later turns.

### Writing memory

**The more memory writes itself, the more it needs watching**
- **Agent writes during chat** (Claude Code, Copilot, Windsurf, Jules): no extra cost, but silent and based on the model's judgment.
- **Background job after idle** (Codex, Warp): catches more, but lags behind and costs usage.
- **Suggested, user approves** (Devin, Augment's review screen): trusted, but adds friction.
- **User-kept files** (Cline memory bank, Kiro, AGENTS.md): fully visible, but they bloat. Cline's own docs keep warning "one page, not a running log" ([Cline](https://docs.cline.bot/features/memory-bank.md)).

### Getting memory into a chat

**Load a short index, fetch the rest when needed**
- **Load everything** (Cline): simple, costs context every chat.
- **Short index plus read on demand** (Claude Code): bounded, and the agent pulls details when it wants them.
- **Search and load relevant memories at start, pull more mid-task** (Warp, Devin triggers): best fit, but needs good search and hides what got loaded.

### Keeping memory honest and safe

**Stale memory and poisoned memory are the two failures**
- **Staleness fixes:** dates on each write (Claude Code), re-checking links plus expiry (Copilot), keeping old facts with an "invalid from" date instead of deleting them (Zep, [paper](https://arxiv.org/pdf/2501.13956.pdf)).
- **Poisoning is real:** one attack got false memories in through ordinary queries over 95% of the time in idealised tests ([Akto summary](https://www.akto.io/blog/memory-poisoning-ai-agents)). Agents that write memory more eagerly are easier to exploit, and standard prompt-injection defences don't cover it ([ICML 2026](https://icml.cc/virtual/2026/67964)).
- **Main way in:** auto-extracting from web pages, files or tool output.

## Ideas for Harness

**1. Harness's edge is having every chat in one local place**
- Harness already stores every chat from every agent in one format, on the user's machine.
- Warp is the only vendor sharing across agents, and only in its cloud. Community tools do cross-agent search, but as add-ons that parse each agent's own files.
- So "Cursor reads what Claude did yesterday" is natural, private and first-party here.
- **Downside:** the edge only lasts while Harness is where the user runs agents. Chats started outside Harness are invisible.

**2. Step 1 as Amp's shape: find, recent, read**
- **Find:** SQLite's ranked word index over the project's chat titles and message text, with stemming. Return short snippets with chat title, date and an id. No new dependency.
- **Recent:** a list of recent chats, optionally by date range (Claude app's second tool).
- **Read:** fetch a chat by id.
- Same project only, trashed chats removed from the index.
- **Cost:** the index adds 8–45% of the text's size on disk.
- **Open question:** what to index. User and assistant text, not tool output, keeps the index small and keeps web or file content out (a poisoning path).

**3. Make the read tool show what happened later**
- **Option A, paged text:** tool calls folded to one line marked worked or failed (Harness has the results), so a failed edit isn't mistaken for a done one. Open the page at the hit, and always include the chat's last few turns, so the agent sees where things ended up. No extra usage. Long chats take several calls and fill the caller's context.
- **Option B, question-focused reader:** a cheap headless model call on the user's subscription, like Amp. Better answers, but costs usage and adds a wait. It also means picking which agent's model does the reading.
- **Option C, middle path:** Claude Code can hand the paging to its own helper agent, which keeps the main chat's context clean at no cost to Harness. Unverified whether Cursor can do the same.

**4. Start with keyword search, and watch for misses before adding meaning search**
- Tell the agent in the tool description to try several wordings.
- Log misses: searches that return nothing, or many retries in a row.
- If misses pile up, add meaning search over one short summary per chat, not every message. The 30 MB static model, loaded only during a search, keeps idle memory near zero.
- **Downside:** the 18% retrieval gap, and one more thing to ship on two platforms.
- **Thin evidence:** nothing tests loose pointing over coding chats specifically.

**5. A per-chat card does three jobs**
- **What:** a short note (what the chat was about, decisions, open questions, files touched), written when the chat goes idle. Harness already shuts down idle agents, so the trigger exists.
- **Three uses:**
  - Loose pointing works, because the card says what the chat was about in plain words.
  - It's raw material for memory.
  - It's the handoff note for step 2.
- **Costs:** usage per chat, and a choice of who writes it. Asking the chat's own agent adds a turn to its history. A headless call needs a model and a subscription. Codex shows why to skip it when usage is low.
- **Staleness:** a card goes stale when the chat resumes, so rewrite it at the next idle.

**6. A card without a model, as a first version**
- Build it from what Harness already has: title, first user message, last assistant message, files touched, dates.
- Free and instant, and it may cover most loose pointing.
- **Downside:** no decisions or open questions, so it's weaker as a handoff note.

**7. Find chats by file**
- Harness sees every tool call, so "chats that touched this file" is a cheap lookup with no model, like Amp's.
- Often the most reliable pointer when the user names a feature.

**8. One search, two users**
- The same index can power a search box for the user in the sidebar.
- Users complain about title-only and exact-match search elsewhere, and Cursor shipped transcript search for this reason.
- **Downside:** more UI in a quiet app. It could live behind a key shortcut.

**9. Let the user point exactly with an @-mention**
- Picking a chat in the composer (Amp, Zed, Codex) skips search entirely when the user knows which one.
- Needs good titles: Cursor users complain of pickers full of "New Chat."
- **Cost:** a small menu.

**10. Harness memory as plain files it owns**
- Markdown per project and one global file, shown and editable in the side panel.
- Each memory carries its source chat and date (Warp, Copilot). A short index loads at chat start for every agent, with a tool for the rest (Claude Code's pattern).
- Cursor hears instructions only at the start of a fresh chat, so the index there is a snapshot. Anything written mid-chat reaches it only through the tool.
- **Cost:** another set of files to keep small. Cline's bloat warnings apply.

**11. Living with Claude Code's own memory**
- Claude Code's memory can be switched off with a setting or environment variable, and its folder can be moved (both documented; not tested by us). That gives three paths:
  - **Turn it off** when Harness launches Claude Code. One memory, but Claude loses its habit of saving as it goes, and Cursor-side writing has to replace it.
  - **Point its folder at Harness's project memory**, and use Claude Code's own format (index plus one file per note) as Harness's format. Claude keeps writing the way it's trained to, and Cursor reads the same files through the Harness tool. Unverified: whether Harness can set the folder per launch without editing the user's settings, and how Claude Code's per-repo grouping maps to Harness projects.
  - **Leave it on and import** now and then. Least work, but two memories drift apart.
- Either way, a one-time import of existing Claude Code memory.

**12. Global preferences for every agent**
- Claude Code's global preferences file reaches only Claude.
- **Option:** Harness keeps one global preferences file and injects it at chat start for every agent. Or it treats Claude's file as the source and injects it only for the others, so Claude doesn't get it twice.
- Cursor reportedly reads AGENTS.md project files, but no global equivalent was found. Not verified.

**13. Keep memory checkable and let it expire**
- Show each write in the chat as a quiet one-line note, like Claude Code's "Saved N memories."
- Copilot-style re-checks against code cost model calls. Claude Code avoids the need by not storing code facts at all. Simpler: store dates, and let unused memories fade after a set time.
- **Downside:** expiry can drop a rare but important fact. Pinning fixes that, but adds a control.

**14. Guard against poisoning by limiting what can write**
- Write memory only from the user's own words, or with the user's approval. Never from web pages, files or tool output.
- Codex's "skip chats that used outside content" is a blunter version. Most Harness chats use the browser or terminal, so it would skip too much.
- **Cost:** fewer memories, and some approval clicks.

**15. Decide what trash means for memory**
- Trashed chats drop out of search. Should memories that came from them go too?
- Claude's app keeps them. Harness's source links make the other choice possible: flag or remove memories whose source chat is trashed.

## Open questions for you

- **How loose is "loose pointing"?** If users usually name a feature or a file, word search plus find-by-file may be enough. If they describe vibes ("the one where we argued about layout"), cards or meaning search matter sooner.
- **Who pays for reading and summaries?** A helper model for reading and a card writer both spend the user's subscription. Is that acceptable silently, behind a setting, or only when asked?
- **Which model does the background work** when a chat was Cursor's but the user also has Claude, or only has one?
- **Should Harness memory replace Claude Code's, take it over, or sit beside it?** That decides how much of Claude Code's tuned behaviour you keep.
- **Should the agent write memory on its own at all,** or only on "remember this" plus approved suggestions? Cursor's retreat and the poisoning research point toward caution. Claude Code and Copilot show silent writing can work when it's visible and dated.
- **How much should the user see?** Search calls, memory loads and writes could each show as one quiet line, or stay hidden.
- **Does step 1 need cards at all,** or do cards arrive with step 2's handoff and pay for both at once?

## What we tested (2026-11-16 and 17)

After the research we tested each choice on real Harness chats before building. The result is the "Earlier chats" feature; how it is built is in `PLAN.md`.

**What a reader needs from an old chat**

Fresh agents were handed one form of a chat and asked what was wanted, decided, dropped and still open.

| What the reader got | Discussion chat (Claude reading) | Build chat (Claude and GPT reading) |
|---|---|---|
| Last three turns | Missed a change of direction and most decisions | Knew the final state, not what was built |
| Outline (user messages, start of each reply) | Got decisions; lost options and open questions, which sit at the end of replies | Claude opened 5 of 12 turns; GPT opened none and missed the main fix |
| Whole conversation, no tool calls | Best | Complete for both |
| Same, plus one line per tool call | not run | Complete, plus a few small facts |

- **Size:** tool calls and their output are about 90% of a saved chat. Without them, 85 of 96 chats were under about 10,000 tokens and the largest about 25,000.
- **Dropped attempts live in the tool calls.** Asked "what if we do x like we tried in y", where x had been tried and reverted, GPT agreed with the premise in 0 of 4 runs on the conversation alone and corrected it in 4 of 4 with one-line tool calls added.
- **A chat written by Cursor** read as well as one written by Claude. The tool lines again added what the replies left out: a failed test run and a package installed outside the project.

**Whether agents notice a reference by themselves**

- **18 runs** (Claude, GPT, Composer; with and without a list of recent chat titles): every model read the right chat for "same deal as the panel tabs" and for "my last chat", and none looked anything up for a question that stood alone. The titles list wasn't needed to notice; it saved the search step.
- **Deep in a chat** (ten unrelated turns first): 9 of 10 right. The miss was Composer with the guidance only in the first message. With the guidance in the tool descriptions, GPT and Composer were 4 of 4.

**What the real app showed that the tests hadn't**

- **Cursor titles a chat from the start of its first prompt.** With Harness's notes first, chats were titled "Harness Agent". The user's words now go first.
- **Cursor hands back no output for Harness's own tools,** only that they succeeded, so Harness keeps its own answers to save.
- **Ranking by the sum of every matching message favours long chats.** A chat titled for the thing ranked fourth behind three long chats. Ranking by how many of the query's words a chat holds, the title counting most, put it first.

**Not tested yet:** a chat long enough to be read in parts, a chat whose context was squeezed, and search by meaning (not built; the agent retries wordings instead).

## Sources

**Vendor docs and changelogs**
- [Claude: chat search and memory](https://support.claude.com/en/articles/11817273-use-claude-s-chat-search-and-memory-to-build-on-previous-context)
- [Claude Code memory](https://code.claude.com/docs/en/memory)
- [Codex memories](https://learn.chatgpt.com/docs/customization/memories?surface=app)
- [Cursor 3.11 changelog](https://cursor.com/changelog/side-chat)
- [Amp threads](https://ampcode.com/docs/threads)
- [Amp: Find Threads](https://ampcode.com/news/find-threads)
- [Amp: Read Bigger Threads](https://ampcode.com/news/read-bigger-threads)
- [Warp Agent Memory](https://docs.warp.dev/agents/agent-memory/)
- [Warp multi-harness blog](https://warp.dev/blog/multi-harness-cloud-agent-orchestration)
- [GitHub Copilot Memory](https://docs.github.com/en/copilot/concepts/agents/copilot-memory)
- [Windsurf memories](https://docs.windsurf.com/windsurf/cascade/memories.md)
- [Devin Knowledge](https://docs.devin.ai/product-guides/knowledge)
- [Cline memory bank](https://docs.cline.bot/features/memory-bank.md)
- [Jules memory](https://jules.google/docs/changelog/2025-09-30)
- [Augment Memory Review](https://www.augmentcode.com/blog/how-we-built-memory-review)
- [Zed agent panel (mirror)](https://git.secluded.site/zed/blob/main/docs/src/ai/agent-panel.md?source=1)
- [Anthropic memory tool](https://platform.claude.com/docs/en/agents-and-tools/tool-use/memory-tool)

**Forums, issues and third-party write-ups**
- [Cursor forum: memories gone](https://forum.cursor.com/t/are-my-memories-gone/144057)
- [Cursor forum: ghost rules](https://forum.cursor.com/t/ghost-user-rules-still-injected-in-agent-context-empty-in-settings-user-tab-not-in-aicontext-personalcontext-logout-login-did-not-fix/164709)
- [Cursor forum: semantic search request](https://forum.cursor.com/t/implement-semantic-search-for-past-chats-as-well/143060)
- [Claude Code issue: title-only search](https://claudeissues.com/issue/50344-vs-code-sidebar-search-sessions-is-title-only-add-full-text-search-over-session)
- [Claude Code issue: substring search](https://claudeissues.com/issue/88516-session-history-search-is-a-raw-substring-match-reordering-two-words-returns-zer)
- [Codex memory deep dive (third-party)](https://codex.danielvaughan.com/2026/04/18/codex-built-in-memory-system-deep-dive/)
- [omnirecall: cross-agent session search](https://scottspence.com/posts/omnirecall-search-claude-code-codex-and-pi-sessions)

**Search and memory evidence**
- [Is Grep All You Need? (arXiv)](https://arxiv.org/abs/2605.15184)
- [Summary of its results table](https://blakecrosley.com/blog/agent-search-is-a-runtime-problem)
- [Letta memory benchmark (vendor)](https://www.letta.com/blog/benchmarking-ai-agent-memory)
- [Zep paper (vendor)](https://arxiv.org/pdf/2501.13956.pdf)
- [Memory poisoning summary](https://www.akto.io/blog/memory-poisoning-ai-agents)
- [MPBench, ICML 2026](https://icml.cc/virtual/2026/67964)

**On-device search tech**
- [SQLite FTS5](https://www.sqlite.org/fts5.html)
- [rusqlite bundled build flags](https://github.com/rusqlite/rusqlite/blob/master/libsqlite3-sys/build.rs)
- [sqlite-vec](https://github.com/asg017/sqlite-vec)
- [model2vec](https://github.com/MinishLab/model2vec)
- [model2vec results](https://github.com/MinishLab/model2vec/blob/main/results/README.md)
- [EmbeddingGemma](https://huggingface.co/blog/embeddinggemma)
