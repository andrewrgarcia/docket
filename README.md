# docket

**Every project and idea you have, in one list, ready to hand to any AI.**

One folder of markdown files. No database, no daemon, no config. The command
is `dk`.

```
  HASH  NAME                STATUS    AGE  DONE      WHAT
  a43b  moxi                active     3d  ▰▰▰▱ 3/4  semantic spatial description language
  7f20  yggdrasil-cli       active     9d  ▰▰▰▰ 4/4  project flattener and diff engine
  eab2  flashcall           stable    13d  ▰▰▰▰ 4/4  high-visibility calling interface for seniors
  d8fe  the-fractal-prince  active   145d  ▱▱▱▱ 0/4  side-scrolling recursive platformer
  0e94  membrane            dead     201d  ▱▱▱▱ 0/4  replaced by docket

5 cards · 1 untouched 90d+ · 11/20 sections written · ~34k tokens
```

`145d` is the point. You forgot about that one.

**AGE** is days since you last touched the project — for a git repository,
the last commit or staged change; for anything else, the last time the card
was edited. docket's own rewrites never count, so a migration cannot make a
forgotten project look fresh.

**DONE** is how much of the card you have actually written: sections with
something in them over sections in total. A project you registered and never
came back to shows `▱▱▱▱ 0/4` and says so at a glance.

**STATUS** is a word, not a menu. `active`, `idea`, `paused`, `done` and
`dead` get a colour and a place in the sort order — done and dead sink to the
bottom and print dim — and anything else you write, `stable` or `shipped` or
`blocked`, is kept and shown as you wrote it.

The line under the table counts what the table cannot: how many projects have
gone untouched for a quarter, how much of your cards is actually filled in,
and what sending the lot would cost in tokens.

Every card has an eight-character hash, fixed for its lifetime, and answers to
any unambiguous prefix of it: `dk a43b`, `dk show d8f`. The column shows the
shortest prefix that is currently unique. Names work too — `dk moxi` — and an
exact name always wins over a hash prefix, so a card called `face` or `abed` is
never shadowed by hex.

The hash is not a checksum of the card. It is generated once at `add` and left
alone, so it survives every rename and rewrite. Cards made before this existed
get one the first time you run `dk`.

## The loop

```bash
dk add .          # register the project you're standing in, README and all
dk pick           # tick the ones that matter, watch the token count, write
dk out            # or skip picking: write every card
```

Both write `DOCKET.md` in the current directory. Drop that file into Claude,
ChatGPT, or anything else. Every card's README rides along, so the model gets
what the project says about itself rather than a one-line summary.

## Install

```bash
cargo install docket-cli
```

Binaries for Linux, macOS and Windows are on the
[releases page](https://github.com/andrewrgarcia/docket/releases). The binary
is `dk` — two letters, because you will type it a lot.

## A card

```markdown
# moxi
id: a43b21c0
status: active
what: semantic spatial description language
path: /home/andrew/code/moxi
readme: /home/andrew/code/moxi/README.md

## now
Rewriting the parser to keep source spans.

## next
[x] M1 — parser keeps source spans
[ ] M2 — error messages that point at the span
[ ] M3 — relative anchors

some notes between the boxes are fine

## open questions
Should `place` accept relative anchors?

## notes
```

Field lines above the first `##`, then whatever you type. Prose, lists,
`[ ]` checkboxes, any mix — the sections are yours. Checkboxes are ordinary
GitHub checklist syntax, so the card renders correctly anywhere.

**The README is linked, not copied.** `readme:` is a path, and the file is
read when you run `show`, `pick` or `out` — so it is never stale, there is
nothing to re-sync, and an AI asked to revise the card cannot rewrite the
project's README along with it. The card stays a page of your own notes.

Everywhere the README appears it is grafted on as a final `## readme`
section, with its own headings nested beneath it, so it can be picked whole
or one heading at a time. If the file has moved, output says
`[no README at …]` rather than quietly leaving it out.

The trade is that a card is no longer self-contained: read the store on a
machine where the project is not checked out and the README is missing. Your
notes, which are the part docket exists to keep, are always there.

Known statuses aside, `dk add .` names the card after the project's own name
— the `name` in `Cargo.toml`, `package.json` or `pyproject.toml` — falling
back to the directory when there is no manifest. A crate in `repo/cli`
becomes `fur-cli`, not `cli`. The filename is the card's identity, so use
`dk rename` to change it; editing the `#` heading by hand does not move it.

`dk add` with no path makes a card with no `path:`. That is an idea, and
ideas are first-class here.

## Several folders, one card

A project often lives in more than one place: the repo, its issue archive, an
eval harness. Keep one card and list the extra folders in its header, under
`path:`:

```
path: /home/andrew/moxilang/moxi
place: issues ~/moxilang/moxi-issues-archive
place: eval ~/moxi-eval
```

`path:` is the primary place, called `main`. Each `place:` line is a label (one
word) and a path (`~/` works). `dk resume moxi` then gives every place its own
code index under `### <label> · <path>` (each from that folder's own
`WHITE.md`), and `dk resume moxi --place eval` keeps just one. `dk here`, run
inside any of the folders, prints the card they belong to, so
`dk resume "$(dk here)"` works from wherever you are. A card with one place
behaves as before.

## Commands

| | |
|---|---|
| `dk` | the list (with several books: the book index) |
| `dk show <name>` | read a card (by name, hash, or a prefix of either) |
| `dk edit <name>` | edit a card in the terminal |
| `dk code [name]` | open a card in VS Code; no name opens the whole store |
| `dk add [path]` | new card; no path means an idea |
| `dk pick` | browse, choose sections, write `DOCKET.md` (alias `dk tree`) |
| `dk out` | write every card to `DOCKET.md` |
| `dk resume <card>` | write `RESUME.md`: the card, its latest sessions, its code |
| `dk here` | print the card that owns the folder you are in |
| `dk rename <old> <new>` | rename a card, heading and all |
| `dk rm <name>` | delete, after you type the name back |
| `dk where` | print the store path |
| `dk book` | list your books; `new`, `add`, `rm`, `use` manage them |
| `dk set <name> <key> <value>` | set a header field, or `state` in `## now` |
| `dk todo <name> <text>` | add an open `[ ]` box to `## next`, in the style of the ones there |
| `dk tick <name> <text>` | tick the one open box containing the text |
| `dk note <name> <text>` | add a paragraph to `## notes` (`--section` for another) |
| `dk write <name> [file\|-]` | replace the whole card, keeping its id |
| `dk undo <name>` | put back the card as it was before the last of these |
| `dk save <name> <entry\|->` | end a session: the entry into the archive, the state onto the card |

`--out <file>` sends `pick`, `out` or `resume` somewhere other than their
default file (`resume --out -` prints it), `--place <label>` limits `resume` to one of the card's folders, and `-b <book>` runs one command in a book other than the
default (see Books). `dk save` has its own (see Sessions, from a shell). An unrecognised word is
treated as a card name, so `dk moxi` works.

Exit codes: `2` misuse, `3` no such card, `4` name already taken, `1`
everything else.

## Picking

`dk pick` (alias `dk tree`) is the whole store as a fold-out tree with a
checkbox beside every card and every heading. It is where you decide what a
model gets to see.

```
▦ dk pick  14 cards · 174 headings · ☑ 3/11

▶ [~] ▾ the-fractal-prince  d8fe  0/4         1.2k tok
  [x] ├── ▸ now                                 1 tok
  [ ] ├── ▸ next                                1 tok
  [ ] ├── ▸ open questions                      4 tok
  [ ] ├── ▸ notes                               2 tok
  [~] └── ▾ readme                           1.2k tok
  [~]     └── ▾ The Fractal Prince           1.2k tok
  [ ]         ├── · How it plays              318 tok
  [x]         ├── · Tech                      204 tok
  [ ]         └── · Getting started            96 tok
  [ ] ▸ moxi  0003  4/4                         0 tok

▦ 2 headings from 1 cards · 71 tok  → DOCKET.md
```

A card's own sections are amber at the top level. Everything a README brought
with it — its `#` title, its `##` headings, their `###` children — nests
underneath in grey, as deep as the document goes.

**How the tree is worked out.** A card's sections are the unbroken run of
level-2 headings starting at the first one. The run ends at the first heading
that is not a fresh `##` — a `#` title, a `###`, or a repeat of one already
used — and never resumes. That last rule is what keeps a README's own
`## now` nested where it belongs instead of posing as a second section of the
card. After the run, headings nest by depth. Headings inside fenced code
blocks are not headings.

Space takes the row under the cursor **and everything beneath it**: a card
takes all of it, `## readme` takes the whole document, `## Install` takes its
`### From source` too. Sending a heading without its children would be a
quietly truncated document. A partly-taken row shows `[~]`.

The token count on the right is what that row and its subtree cost, coloured
on the same thresholds `ygg` uses: grey under 200, green under 1k, amber
under 4k, red above. The footer totals the selection, card headers included.

| key | |
|---|---|
| `↑↓` `jk`, mouse | move |
| `space`, click the box | take / drop, with everything under it |
| `enter` `→` `l`, click the row | open |
| `←` `h` | close, or jump to the parent |
| `a` | take everything / nothing |
| `c` | copy the selection to the clipboard |
| `p` | print it to `DOCKET.md` |
| `z` | pack it as `DOCKET.zip`, one markdown file per card |
| `q` `Esc` | leave |

`c` uses `wl-copy`, `xclip`, `xsel`, `pbcopy` or `clip.exe`, and falls back to
OSC 52 so it works over SSH — that route is size-capped and silently dropped
by some terminals, so it is reported as attempted rather than done.

`z` writes an archive holding `INDEX.md` and one `<card>.md` per chosen card,
for the models that would rather read files than one long paste.

Picking a heading writes the card's header with it, so `## now` never arrives
without the card it belongs to, and chosen headings are emitted in document
order. Nothing is remembered between runs.

`--out <file>` redirects `p` and `z` as well as `dk out`.

`dk out` skips the screen and writes every card whole.

## Resume

```bash
dk resume moxi        # writes RESUME.md, prints its path
```

One file for picking a project back up in any chat or agent: the card's own
sections, the newest three session entries whole (older ones as one index line
each), and an index of the code named by the project's `WHITE.md` (file, lines, tokens),
made by [yggdrasil](https://github.com/andrewrgarcia/yggdrasil-cli). The files
themselves are not in it: an agent with the repo opens what it needs, and a chat
that cannot gets them by pasting `ygg --white WHITE.md --contents` beside the
resume. The README is not listed — put `README.md` in `WHITE.md` if you want it. What each part costs
prints to stderr before you paste it.

Sessions are [fur](https://github.com/fur-labs/fur-cli) conversations kept in
`sessions/` inside the store and tagged `dk-<card id>`. Long-form documents kept there (plans, option analyses) are
listed by name, title, status and cost, never inlined. Whatever ends a session — you, Claude Code, a Cowork run —
saves an entry there, with `dk save` or by hand. The format, and what an entry must contain, is the
[Salvation spec](https://github.com/andrewrgarcia/salvation/blob/main/SPEC.md).

`dk resume moxi --out -` prints the file instead of writing it, for an agent
that reads it straight from the shell.

## Sessions, from a shell

An agent with a shell — Claude Code, a Cowork session with a terminal, Codex —
can keep a card current and close a session without anyone opening an editor:

```bash
dk set moxi state "P0 pipeline · render task done, glTF step remaining"
dk todo moxi "emissive materials → MTL Ke"
dk tick moxi "P0 pipeline"
dk note moxi "obj2gltf keeps one node per part"        # ## notes
dk note moxi --section "open questions" "which model for v2?"

dk save moxi entry.md --doc plan.md --tick "P0 pipeline" --next "P1 materials"
cat entry.md | dk save moxi -                          # stdin works too
```

`dk save` checks the entry has its title and all seven sections in order (and
adds the `<!-- dk:session v1 -->` line if it was left off), finds the card's
sessions conversation by its tag or creates it, names the files
(`SES-<UTC stamp>.md`; a document keeps a `DOC-YYYYMMDD-slug.md` name or gets
one from its title, and a document already there is revised in place), appends
one fur marker per new file, sets the card's `state:` from the entry's
`## state`, ticks and adds what the flags say, and reads every file back. A bad
`--tick` or a malformed entry stops it before anything is written.
`--dry-run` says what it would do.

Every one of these keeps the card's previous version in `.undo/` inside the
store; `dk undo moxi` swaps it back (and again, forward). `dk write moxi -`
replaces a whole card from stdin and keeps its id. None of them ever touches
`## readme`, and `rm` still makes you type the name.

Anything missing shows up as a bracketed line, never as silence: `[no WHITE.md
at …]`, `[ygg not found — install yggdrasil-cli]`, `[no sessions yet]`. A card
may set `white: <file>` to name a manifest other than the project's `WHITE.md`.

## Editing

`dk edit` opens an editor built into docket — the same on Linux, macOS and
Windows, with nothing to install.

It has modes, like vi, and the bottom bar always says which one you are in and
what the keys do:

- **command mode**, where you land. Arrows move. `e` starts typing. `:` opens
  the command line. `u` undoes, `d` cuts a line, `p` puts it back.
  **Space ticks the `[ ]` box on the current line; Tab jumps to the next
  box, Shift-Tab to the previous.** Open boxes are green, done ones dim, so a
  card's state is visible before you read it.
- **insert mode**. You type. `Esc` returns to command mode. `Ctrl-S` saves.
- **the `:` line**: `:w` saves, `:q` quits, `:q!` discards, `:wq` and `:x` save
  and quit.

Headings, field lines and the README section are coloured differently, so the
shape of a card is visible at a glance. Saves are atomic — written beside the
card and renamed into place — so an interrupted save leaves the old card
intact.

`dk code moxi` opens the card in VS Code. `dk code` with no card opens the
whole store as a folder, which is the better one most days: every card in one
sidebar, with search and multi-file edit across all of them.

It tries `code`, `codium`, `cursor`, `windsurf` and `code-insiders`, in that
order. `DOCKET_CODE` names something else — it takes arguments, so
`DOCKET_CODE="subl -n"` works.

Set `DOCKET_EDITOR`, `VISUAL` or `EDITOR` (checked in that order) to use your
own editor for `dk edit` instead. `EDITOR="code --wait"` works; the `--wait` matters, or the
editor returns before you have typed anything. `DOCKET_EDITOR=builtin` forces
docket's when `EDITOR` is set for other tools.

## Colour

On by default, off when the output is piped, off when `NO_COLOR` is set, off
when `TERM=dumb`. `dk show moxi > card.md` writes the card, not a screenshot
of it.

## Books

A book is a store: a folder of cards, with `sessions/` inside. One book is all
most people need, and it is what you have until you ask for a second. A second
is for keeping collections apart: personal and work, a team's cards in their
own git repo, or a demo with nothing private in it.

```bash
dk book                          # list them: cards, active, newest, path
dk book new bcrp                 # a new, empty book (or: dk book new bcrp ~/work/bcrp)
dk book add ~/notes/docket       # register a folder of cards you already have
dk book use bcrp                 # make it the default
dk book rm bcrp                  # forget it; the folder is left alone
```

The first time you make a book, the store you already have is registered too,
named after its folder, and stays the default, so starting a second collection
never makes the first one disappear.

With two or more books, a bare `dk` opens the book index: the default is under
the cursor, Enter shows that book's cards, `p` picks from it, `q` quits. Every
other command uses the default book unless you say otherwise:

```bash
dk -b bcrp                       # one command in another book
dk show bcrp/rates               # a card by book/name
dk resume bcrp/rates
DOCKET_BOOK=bcrp dk              # for a whole shell
```

Nothing remembers which book you looked at last, on purpose: a sticky choice is
how a card ends up in the wrong collection. The default is the one you set
with `dk book use`, and every list names its book.

Which book a command uses, first match wins: `-b` or `book/card`,
`DOCKET_BOOK`, `DOCKET_HOME`, the default book (or the only one), and with no
books registered, the single store. The books are named in a small file you may
edit by hand: `~/.config/docket/books.toml` (`DOCKET_CONFIG` points elsewhere).

`DOCKET_HOME` still works and still wins over the default book. That is for
people with one store who set it long ago. If you use books, do not set it:
it would hide the others.

## Where things live

The platform data directory: `~/.local/share/docket`,
`~/Library/Application Support/docket`, `%APPDATA%\docket`. `dk where` prints
it. With one store, set `DOCKET_HOME` to move it; with books, register the
folder you want (`dk book add`). New books made without a path go in
`docket-books/` beside it.

It is a flat folder of markdown files, one per card, and nothing else. Nothing
is ever written inside your projects — a card points at a path; it never
touches it.

## Keep the store in a private git repo

Recommended, and the format was chosen for it.

```bash
mkdir -p ~/notes                     # the parent, not the target
mv "$(dk where)" ~/notes/docket      # must not already exist, or mv nests it
dk book add ~/notes/docket docket    # tell docket where it went
cd ~/notes/docket && git init && git add -A && git commit -m "docket store"
```

`dk where` prints the current store, so that first move works wherever your
platform put it. (`export DOCKET_HOME=~/notes/docket` in your shell profile
also works for one store, but it overrides every book, so books are the better
habit.) Each book can be its own git repo.

Your notes are the only copy. There is no undo and `dk rm` is permanent, so
git turns *gone* into one command back. And because a card is markdown and
its hash never changes, `git log -p moxi.md` is a readable history of how you
thought about that project over six months — which is the record docket
exists to keep.

Private rather than public: a card holds your open questions and your notes on
unfinished work, plus absolute paths that show your directory layout. None of
that is secret, but none of it is something you meant to publish either.

docket never calls git. Committing is a decision, and a tool that quietly
committed your half-written notes would be worse than one that does nothing.
Expect one noisy commit the first time you run a new version, since the id
backfill rewrites cards that predate it.

**Don't use Dropbox or iCloud for this instead.** There is no locking, so two
machines editing the same card produces a conflicted copy and no warning. Git
gives you a conflict you can see.

## What docket is not

**Not a repo packer.** It never reads your source — only the README, once, at
`add`. For code in the prompt use
[yggdrasil](https://github.com/andrewrgarcia/yggdrasil-cli) and hand over both
files.

**Not agent memory.** No daemon, no MCP server, no session capture, no
retrieval. `dk save` files an entry someone wrote; it never decides what goes
in one. You choose what the model sees, every time.

**Not a task manager.** No due dates, no priorities, no boards.

**Not clever.** No tags, no search, no sync. At twenty cards, `dk` prints
twenty lines and your eyes do the searching.

Undo is one step deep and covers the writing verbs (`set`, `todo`, `tick`,
`note`, `write`, `save`). `rm` has none, which is why it makes you type the name.

## Development

```bash
makers ci        # tests, correctness lints, release build
makers fmt       # rustfmt, if you want it — never run automatically
```

cargo-make, not GNU make: the task file has to work on Windows too.

CI runs the tests on Linux, macOS and Windows, a clippy pass limited to
correctness and footguns, and a build against the minimum Rust version. It
does not check formatting and does not fail on style lints. A red badge means
something is broken.

The tests are the specification. `tests/cli.rs` drives the real binary against
a throwaway `DOCKET_HOME` and books file; unit tests cover the buffer, the wrapping, the
syntax classes, the card format and the output document. No dev-dependencies.

## License

MIT
