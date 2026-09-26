# docket

**Every project and idea you have, in one list, ready to hand to any AI.**

One folder of markdown files. No database, no daemon, no config. The command
is `dk`.

```
  HASH  NAME           STATUS    AGE  DONE      WHAT
  a43b  moxi           active     3d  ▰▰▰▱ 3/4  semantic spatial description language
  7f20  yggdrasil-cli  active     9d  ▰▰▰▰ 4/4  project flattener and diff engine
  c118  kol-game       idea     142d  ▰▱▱▱ 1/4  retro whimsical browser game
  0e94  membrane       dead     201d  ▱▱▱▱ 0/4  replaced by docket

4 cards
```

`142d` is the point. You forgot about that one.

**AGE** is days since you last touched the project — for a git repository,
the last commit or staged change; for anything else, the last time the card
was edited. docket's own rewrites never count, so `sync` cannot make a
forgotten project look fresh.

**DONE** is how much of the card you have actually written: sections with
something in them over sections in total. An idea you jotted and abandoned
shows `▱▱▱▱ 0/4` and says so at a glance.

Every card has an eight-character hash, fixed for its lifetime, and answers to
any unambiguous prefix of it: `dk a43b`, `dk show 7f2`. The column shows the
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
status: active
what: semantic spatial description language
path: /home/andrew/code/moxi

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

## readme
<the project's README, whole>
```

Field lines above the first `##`, then whatever you type. Prose, lists,
`[ ]` checkboxes, any mix — the sections are yours. Checkboxes are ordinary
GitHub checklist syntax, so the card renders correctly anywhere. The README always
sits last, because the hand-written sections are what you came for and the
README is reference.

Known statuses are `active`, `idea`, `paused`, `done` and `dead`; the last two
sort to the bottom and print dim. Anything else is kept but shown as `-`.

`dk add .` names the card after the project's own name — the `name` in
`Cargo.toml`, `package.json` or `pyproject.toml` — falling back to the
directory only when there is no manifest. A crate in `repo/cli` becomes
`fur-cli`, not `cli`. The filename is the card's identity, so use
`dk rename` to change it; editing the `#` heading by hand does not move it.

`dk add` with no path makes a card with no `path:`. That is an idea, and
ideas are first-class here.

The README is captured once, at `add`. When a project's README moves on, `dk
sync` re-reads it — every card with a path, or one named card. Sync replaces
the `## readme` section and touches nothing above it, so your own notes are
never at risk. It is a command you run, never something that happens on its
own: a card that rewrites itself is a card you stop trusting.

## Commands

| | |
|---|---|
| `dk` | the list |
| `dk show <name>` | read a card (by name, hash, or a prefix of either) |
| `dk edit <name>` | edit a card in the terminal |
| `dk open [name]` | open a card in your desktop editor; no name opens the folder |
| `dk add [path]` | new card; no path means an idea |
| `dk sync [name]` | re-read project READMEs into their cards |
| `dk pick` | browse, choose sections, write `DOCKET.md` (alias `dk tree`) |
| `dk out` | write every card to `DOCKET.md` |
| `dk rename <old> <new>` | rename a card, heading and all |
| `dk rm <name>` | delete, after you type the name back |

`--out <file>` sends `pick` or `out` somewhere other than `DOCKET.md`. That is
the only flag. An unrecognised word is treated as a card name, so `dk moxi`
works.

Exit codes: `2` misuse, `3` no such card, `4` name already taken, `1`
everything else.

## Picking

`dk pick` (alias `dk tree`) is the whole store as a fold-out tree with a
checkbox beside every card and every heading. It is where you decide what a
model gets to see.

```
▦ dk pick  14 cards · 174 headings · ☑ 3/11

▶ [~] ▾ docket-cli  133f  0/4              412 tok
  [x] ├── ▸ now                              1 tok
  [ ] ├── ▸ next                             1 tok
  [ ] ├── ▸ open questions                   4 tok
  [ ] ├── ▸ notes                            2 tok
  [~] └── ▾ readme                        2.1k tok
  [~]     └── ▾ docket                     2.1k tok
  [ ]         ├── · The loop                112 tok
  [x]         ├── · Install                  57 tok
  [ ]         └── · Commands                208 tok
  [ ] ▸ moxi  0003  4/4                      0 tok

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
| `o` | open every card to its own sections |
| `*` | open or shut everything |
| `z` | collapse |
| `t` | tick the checkbox on this line, written to the card at once |
| `N` `X` `R` | take every `now` / `next` / `readme` — card sections only |
| `a` `n` | take everything / nothing |
| `e` | edit this card, then come back |
| `w` | write the file |
| `q` `Esc` | leave |

Picking a heading writes the card's header with it, so `## now` never arrives
without the card it belongs to, and chosen headings are emitted in document
order. Nothing is remembered between runs.

`dk out` skips the screen and writes every card whole.

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

`dk open moxi` hands the file to whatever your desktop opens markdown with —
gedit, TextEdit, VS Code, Typora. `xdg-open` on Linux, `open` on macOS,
`start` on Windows, with `gio`, `kde-open` and `wslview` as fallbacks. Set
`DOCKET_OPENER` to name a program yourself. `dk open` with no card opens the
store folder in your file manager.

Set `DOCKET_EDITOR`, `VISUAL` or `EDITOR` (checked in that order) to use your
own editor for `dk edit` instead. `EDITOR="code --wait"` works; the `--wait` matters, or the
editor returns before you have typed anything. `DOCKET_EDITOR=builtin` forces
docket's when `EDITOR` is set for other tools.

## Colour

On by default, off when the output is piped, off when `NO_COLOR` is set, off
when `TERM=dumb`. `dk show moxi > card.md` writes the card, not a screenshot
of it.

## Where things live

The platform data directory: `~/.local/share/docket`,
`~/Library/Application Support/docket`, `%APPDATA%\docket`. Set `DOCKET_HOME`
to move it, which is how you keep the store in a git repo or a synced folder.

Nothing is ever written inside your projects. A card points at a path; it
never touches it.

## What docket is not

**Not a repo packer.** It never reads your source — only the README, once, at
`add`. For code in the prompt use
[yggdrasil](https://github.com/andrewrgarcia/yggdrasil-cli) and hand over both
files.

**Not agent memory.** No daemon, no MCP server, no session capture, no
retrieval. You choose what the model sees, by hand, every time.

**Not a task manager.** No due dates, no priorities, no boards.

**Not clever.** No tags, no search, no sync. At twenty cards, `dk` prints
twenty lines and your eyes do the searching.

There is no undo. `rm` makes you type the name for that reason.

## Development

```bash
cargo make ci      # fmt, clippy -D warnings, tests, release build
```

cargo-make, not GNU make: the task file has to work on Windows too.

The tests are the specification. `tests/cli.rs` drives the real binary against
a throwaway `DOCKET_HOME`; unit tests cover the buffer, the wrapping, the
syntax classes, the card format and the output document. No dev-dependencies.

## License

MIT
