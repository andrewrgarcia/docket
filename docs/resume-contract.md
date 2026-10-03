# resume contract — v1 (draft)

How a session's reasoning is saved when it ends, and how the next session,
in any harness, picks it up. **This document is the interface.** `dk resume`
is one reader of it. A Cowork session with the docket store connected is
another, and needs no binary at all. Anything here can be written by hand.

Five decisions, numbered so they can be approved one at a time.

**Books.** Where this document says "the docket store" it means one *book*: a
folder of cards with `sessions/` inside, exactly as D1 describes it. With
several books, each has its own `sessions/`, and a card's sessions live in the
book that holds the card. A command or an agent works inside one book at a
time; `dk where -b <name>` prints that book's folder, and in Cowork the
connected folder is the book. Nothing in D1-D5 changes inside a book.

---

## D1 — Sessions live in the docket store, as a fur archive

```
<the book>/                          ← $DOCKET_HOME, or a registered book's folder
├── moxi.md                          ← cards, as today (top-level *.md only)
├── yggdrasil-cli.md
└── sessions/                        ← a fur project root
    └── chats/
        └── moxi-sessions-3f2a91c4/
            ├── convo.md             ← fur spine, one per card
            ├── SES-20260930-221400.md
            └── SES-20261002-093112.md
```

- `Store::cards()` reads only top-level `*.md`, so `sessions/` is invisible
  to every existing command. No card can be created by accident.
- One private git repo holds both the cards and the reasoning. One connected
  folder in Cowork gives an agent all of it.
- Public repos (moxi, ygg, fur) never get session logs committed into them.
- `cd "$(dk where)/sessions" && fur search "spans"` searches every session of
  every project.

*Rejected:* a `chats/` folder inside each project repo — it would publish
private reasoning from public repos, and put dk in the business of writing
into projects, which the README promises it never does.

## D2 — A card finds its sessions by tag, not by path

The conversation for a card is the one whose front matter carries the tag
`dk-<card id>`:

```yaml
---
fur_schema: 1
conversation_id: 3f2a91c4-0b7e-4c1d-9a55-2e6f0c8d1b37
title: moxi sessions
created_at: 2026-09-30T22:14:00Z
tags:
  - dk-a43b21c0
  - session
---
```

- The card id is fixed for the card's lifetime. The folder name, the title
  and the card name are not: `dk rename` and fur's folder sync both change
  names, and neither can break this link.
- No new card field. Nothing to backfill.
- Folder name follows fur's own rule, `<slug(title)>-<first 8 of id>`, so fur
  never wants to rename it.
- Zero conversations tagged → "no sessions yet". Two or more → error naming
  both folders. Never a guess.

*Rejected:* a `fur:` path field on the card. Paths rot on rename, and every
existing card would need one.

## D3 — The code slice is the project's `WHITE.md`

- `dk resume` uses `<path>/WHITE.md` if it exists. An optional `white:` card
  field overrides it for projects that keep the manifest elsewhere.
- The manifest is the single knob for which files an agent is told about.
  Want the README listed in the resume? Put `README.md` in `WHITE.md`.
- ygg runs with the project directory as working directory, with
  `--white <manifest> --out <temp>.md`, and dk inlines the result: ygg's
  index (path, lines, words, tokens per file), **not the file contents**.
  An agent with the repo connected opens a file when the task needs it and
  gets the current version. A chat that cannot read the repo is handed the
  contents by pasting ygg's own codex (`ygg --white WHITE.md --contents`)
  beside the resume.

*Rejected:* inlining the contents by default. Measured on dk's own manifest,
the contents were about two thirds of the resume (7.4k of roughly 11k
tokens), paid on every resume whether or not the task touched code, and stale
the moment the code changed.
- **Places.** A card may live in several folders: `path:` is the primary one,
  labelled `main`, and each `place: <label> <path>` header line adds another
  (the label is one word, the path is the rest of the line). With more than
  one place the `## code` part holds one `### <label> · <path>` subsection per
  place, `main` first, each built as above from that folder's own `WHITE.md`;
  `white:` applies to `main` only. A card with one place has no subsections,
  so a resume written before places existed reads the same. `--place <label>`
  keeps one subsection. A place whose folder is gone gives its own bracketed
  line and costs the others nothing.
- Failures read like the missing-README note, never an omission:
  `[no WHITE.md at …]`, `[ygg not found — install yggdrasil-cli]`,
  `[ygg failed: <first line of stderr>]`.

## D4 — One session entry: fixed headings, every choice gives its reason

A session ends by writing `SES-YYYYMMDD-HHMMSS.md` (UTC) next to `convo.md`:

```markdown
<!-- dk:session v1 -->
# moxi · 2026-09-30 22:14 · cowork · opus-5.5 high

## done
- parser keeps source spans through the lowering pass

## decided
- spans stored as byte offsets, not line/col — <why>

## rejected
- carrying spans in a side table keyed by node id — <why not>

## state
M2 · 2 of 3 criteria met; error rendering for nested spans remaining

## blockers
none

## next
M2 finish · Sonnet 5 extended · tests gate every criterion

## files
- src/parser/span.rs — new
```

Rules:

- All eight headings, always, in this order. An empty one says `none`, so a
  missing heading means "forgot", never "nothing happened".
- Every `decided` and `rejected` line carries its reason after ` — `.
  **`rejected` is the reason this loop exists.** It is the reasoning a fresh
  session cannot rebuild from the code.
- `state`, `blockers` and `next` are session-pilot's SESSION CLOSE footer,
  field for field. The footer is still printed in chat. The entry is what
  survives the chat.

Then one marker line is appended to `convo.md`:

```
<!-- fur:msg id=<uuid v4> avatar=claude ts=<RFC3339 UTC> link=SES-20260930-221400.md -->
```

- `sha256=` is added when the writer can compute it, and omitted otherwise.
  fur treats it as optional.
- The first session for a card creates the folder and `convo.md` (D2).
- fur's `.fur/` index will not see new entries until
  `fur rebuild --force` is run in `sessions/`. dk never reads `.fur/`, so
  this affects fur commands only. (fur follow-up, out of scope.)

**Card write-back.** At close, the agent may change three things in the card
and nothing else:

- **One `state:` line in `## now`.** The first line of `## now` that starts
  with `state:` is replaced with `state: <the entry's state>`. If there is
  none, one is inserted as the section's first line. Nothing else in `## now`
  is touched — it holds your own prose and checklist, and an earlier draft of
  this rule, "replace the body", would have deleted both (found by the first
  real close, 2026-10-01).
- **Tick boxes** `[ ]` → `[x]` for items this session finished, matched by
  their existing text. Never reword a box.
- **Append** new `[ ]` items to the end of `## next` for things discovered.

Header fields, other sections and the README link are never touched. The
card's age resets, which is correct, because you worked on it.

### Documents: long-form output

A session entry is a ledger, kept short because the newest three are loaded
whole every time. When a session produces reasoning worth rereading in full —
an option analysis, a phased plan, a spec still in draft — it goes into a
**document** instead: a linked file in the same conversation, with the same
marker line, named `DOC-YYYYMMDD-<slug>.md`:

```markdown
<!-- dk:doc v1 -->
# Unified workflow — survey and design
status: adopted
```

- `status:` is required: `draft`, `adopted` or `superseded`. Plans go stale,
  and a superseded plan read as current is worse than none.
- A document may be revised in place; the store is a git repo. When a new
  document replaces an old one, the old one's `status:` line becomes
  `superseded` — the only edit ever made to it.
- The entry still records the conclusion, pointing at the document:
  `- dk is the hub — see DOC-20261001-workflow-design`.
- The test for writing one: if a `decided` or `rejected` line cannot hold its
  reason in one line, the reason belongs in a document.
- A spec the *code* will obey belongs in the project repo instead (like this
  file), where it is versioned with the code it governs. Documents are for
  reasoning, which stays private.

## D5 — `RESUME.md`: one file, ordered by what can't be rebuilt

`dk resume <card> [--out FILE]` writes `RESUME.md` (default, in the current
directory):

```markdown
<!-- dk:resume v1 -->
# RESUME · moxi

## card
<the card: header and its own sections — README excluded, see D3>

## sessions
<newest 3 entries, whole, newest first>

### documents
- DOC-20261001-workflow-design · Unified workflow · adopted · 3.8k tok · 2026-10-01
- …one line per document, newest first: file, title, status, cost, date

### earlier
- SES-20260901-110233 · M1 parser spans · 2026-09-01
- …one line per older entry: file, first line of its `next`, date

## code
<ygg's index of WHITE.md — which files, how big — or the bracketed note from D3>
```

- **Order is by replaceability.** The card and sessions can't be rebuilt
  from anything else; code can. A model that truncates loses the cheapest
  part.
- **No budget flag.** Section token counts and the total print to stderr on
  the picker's heat scale. Show the cost before it is paid; the manifest is
  the control. `--out` stays the only flag.
- Older sessions appear as an index, the same idea as DOCKET's INDEX and
  ygg's, so an agent can ask for one by name.
- Refuses to write into the store root, because a `.md` file there would
  become a card. Compared by real path, so `../store/RESUME.md` is caught too.
- stdout carries the path written and nothing else.
- Entries are separated by a `---` rule. `### earlier` appears only when
  there is something older than the newest three.
- **Documents are listed, never inlined**, however recent — an agent sees
  that one exists, what it costs and whether it is current, and asks for it
  by name. A spec the code obeys belongs in the repo instead, and listed in
  `WHITE.md` it shows in every resume's code index. Documents do not count
  toward the three-entry window.
- Only a linked file whose first line is `<!-- dk:session` is a session, and
  only one starting `<!-- dk:doc` is a document.
  Other messages in the conversation (`fur jot` chatter, unlinked messages)
  are passed over without comment.
- A link that is missing on disk, unreadable, or leaves the conversation
  folder is reported as a bracketed line at the top of `## sessions`, and is
  never read. A `convo.md` that is not valid text (an encrypted archive) is an
  error that says so, not "no sessions yet".
- A `code` failure never loses the card: `ygg` missing, failing, or silent
  becomes a bracketed line and the file is still written.

---

## Who reads and writes what

| actor | reads | writes |
|---|---|---|
| `dk resume` | card, `sessions/chats/*/convo.md` + links, `WHITE.md` via ygg | `RESUME.md` |
| Cowork, store connected | card + sessions directly; code via a connected repo or a pasted codex | session entry, marker, card write-back |
| Claude Code | `dk resume` output | same as Cowork, with plain file writes |
| plain chat | pasted `RESUME.md` | nothing; prints the footer and the entry for you to save |
| fur | `sessions/` as an ordinary archive | — (reads only) |

## Not in v1

- GitHub Action publishing a per-repo resume for computer-less sessions.
- `dk pick` showing sessions as pickable nodes.
- fur detecting that `convo.md` is newer than its index.
