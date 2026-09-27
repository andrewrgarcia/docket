# Changelog

All notable changes to docket. Format follows [Keep a Changelog]; versions
follow [Semantic Versioning].

## [Unreleased]

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
  README as the card's last section.
- `dk sync [name]` — re-read project READMEs into their cards, replacing
  only the `## readme` section.
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
