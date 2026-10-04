# Changelog

All notable changes to docket. Format follows [Keep a Changelog]; versions
follow [Semantic Versioning].

## [0.2.0]

### Changed
- Bare `dk` lists the current book's cards. The interactive book index is
  gone. `dk book <name|hash>` makes a book current (it sets the default; `dk
  book use` still works), and the choice sticks until the next switch. With
  several books and none current, `dk` prints the book list and how to pick.
- `dk book` lists a HASH column: eight hex characters made from the book's
  name, so they are stable across machines. Books, like cards, resolve by
  name, hash, or a unique prefix of either, including in `-b`.
- `dk show` in a terminal opens a fold view. Every heading folds, from the
  card's `# name` down through the README's own headings. The colours are
  the same, the README starts folded, and `z` gives the outline, `o` opens
  everything and `e` edits and returns. Piped, it prints the plain card as
  before.

### Removed
- The DONE column (`▰▰▱▱ 2/4`) and the "n/m sections written" count under
  the list.
- `dk <card>` as a shortcut for `dk show <card>`. An unrecognised word is now
  a usage error that names `dk show`.

### Added
- `dk resume`: each place's `## code` block opens with a `git:` line read
  fresh from git — branch, last commit date and subject, uncommitted change
  count (or `clean`), ahead/behind its remote. Printed even without a
  `WHITE.md`; failures are bracketed (`[not a git repository]`, `[git not
  found — commit state unknown]`). Commit state changes between sessions, so
  the resume reads it instead of trusting a card or entry that says
  "uncommitted" (Salvation spec D4/D5, 2026-10-04).
- Writing without an editor, for scripts and AI sessions: `dk set <card> <key>
  <value>` (a header field, `place <label> <path>` by label, or `state` in
  `## now`), `dk todo` (an open box at the end of `## next`, bulleted or not to match the boxes already there), `dk tick` (the one
  open box containing the text; two candidates is an error that lists them),
  `dk note` (a paragraph in `## notes`, or `--section <heading>`, made if
  missing), `dk write` (a whole card from a file or stdin, id kept). A text of
  `-` is read from stdin. None of them touches `## readme`.
- `dk undo <card>`: each writing verb keeps the previous version in `.undo/`;
  undo swaps it back, so undoing twice is redo.
- `dk save <card> <entry|->`: ends a session per the Salvation spec — checks
  the entry's shape, finds or creates the card's sessions conversation, names
  `SES-`/`DOC-` files, appends fur markers, sets the card's `state:` from the
  entry, applies `--tick`/`--next`, and reads everything back. `--doc` adds
  documents (an existing one is revised in place, no new marker); `--dry-run`
  writes nothing. Validation happens before any write.
- `dk resume --out -` prints `RESUME.md` to stdout.
- `dk resume <card>` — writes `RESUME.md`: the card's own sections, the newest
  three session entries whole with older ones as an index line each, and the
  index `ygg` makes of the project's `WHITE.md` (which files, how big, not
  their contents). Sessions are read from a
  fur archive in `<store>/sessions/`, found by the tag `dk-<card id>`. Missing
  pieces are bracketed lines, not omissions. Token cost per part goes to
  stderr; the path written goes to stdout. Refuses to write into the store.
  See `docs/resume-contract.md`.
- Long-form documents: a linked file starting `<!-- dk:doc v1 -->` in a
  card's sessions conversation is listed under `### documents` in
  `RESUME.md` — title, status, cost, date — and never inlined.
- Books: several separate collections of cards. `dk book` lists them and
  `new`, `add`, `rm`, `use` manage them; `-b <book>`, `book/card` names and
  `DOCKET_BOOK` pick one for a command; with two or more books a bare `dk`
  opens an interactive index (Enter lists a book, `p` picks from it). The
  first book made registers the existing store as the default. Books are named
  in `books.toml` in the config directory (`DOCKET_CONFIG`). With no books
  registered nothing changes. `DOCKET_HOME` still wins over the default book,
  and `dk book` says so when it is set.
- Places: a project that lives in several folders (a repo, its issue archive,
  an eval harness) keeps one card. `path:` stays the primary place, called
  `main`; extra folders are `place: <label> <path>` lines in the header
  (`~/` allowed). With more than one place, `dk resume` gives each its own
  `### <label> · <path>` code index, and `--place <label>` limits it to one.
  `dk here` prints the card that owns the folder you are in (the deepest place
  wins). A card with a single place reads exactly as before.
- `white:` card field — names a `ygg` manifest other than the project's
  `WHITE.md`.

## [0.1.0]

First release. The binary is `dk`.

### Added
- `dk` — the list: hash, name, status, age, a completion bar, and one line of
  description, coloured and clipped to the terminal width. Age is days since
  the project's last git activity (or the card's last edit); docket's own
  rewrites preserve it.
- Cards are parsed into a heading outline: the card's own sections are the
  unbroken run of level-2 headings from the first one, and everything after
  it — a README's title, headings and subheadings — nests by depth. Fenced
  code blocks are not scanned for headings.
- `dk pick` (aliases `dk p`, `dk tree`, `dk t`) — the store as a fold-out
  tree, selectable at card, section or sub-heading level, a README's own
  structure included. Mouse
  support, per-row token cost with heat colouring, per-card completion,
  and `c` / `p` / `z` to copy, print or pack the selection. The key legend
  shrinks to fit a narrow terminal rather than being cut off.
- Checkboxes: `[ ]` / `[x]` lines in a card's own sections are recognised,
  tallied in the list, coloured in the editor, and ticked with Space in the
  editor's command mode. Tab and Shift-Tab jump between them.
- A stable eight-character hash per card, written into the file at `add` and
  backfilled for older cards. Any card can be addressed by name, by hash, or
  by an unambiguous prefix of either; an exact name always wins.
- `dk code [name]` — open a card in VS Code, or the whole store as a folder
  when no card is named. Tries `code`, `codium`, `cursor`, `windsurf` and
  `code-insiders`; `DOCKET_CODE` overrides.
- `dk add [path]` — register a project or create an idea. Names the card from
  the manifest's own `name`, derives `what` from its description or the
  README's first prose line, notes an `AGENTS.md`, and captures the whole
  README by path. The file is read when a brief is written, so it is never
  stale and never copied into the card; stores holding an embedded
  `## readme` section are migrated to a link the first time they are read.
- `dk show <name>` — read a card, coloured on a terminal and verbatim when
  piped. A bare `dk <name>` does the same.
- `dk edit <name>` — a built-in modal editor, identical on Linux, macOS and
  Windows: `e` to type, `:w` `:q` `:q!` `:wq` to act, colour by line kind,
  soft wrap, grouped undo, line cut and paste, bracketed paste, atomic saves.
  `DOCKET_EDITOR`, `VISUAL` or `EDITOR` selects an external editor instead.
- `dk out` — write every card to the same file.
- `--out <file>` — write somewhere other than `DOCKET.md`.
- `dk rename <old> <new>` — rename a card and its heading together.
- `dk rm <name>` — delete, confirmed by typing the name.
- Colour throughout, disabled when piped, under `NO_COLOR`, or on `TERM=dumb`.

[Keep a Changelog]: https://keepachangelog.com/en/1.1.0/
[Semantic Versioning]: https://semver.org/spec/v2.0.0.html
