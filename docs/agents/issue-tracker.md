# Issue tracker: the ordna board

Issues for this fork are **cards** on ordna boards: one Markdown file per card, one board per area under `context/board/<area>/` (git-ignored working memory; each board's `.ordna/config.yaml` holds the columns and its id prefix). The `ordna` skill reads, creates and moves cards; run it inside the board's folder (`cd context/board/zombies && ordna ls`) and use it instead of editing frontmatter by hand.

| Board | Prefix | Holds |
|---|---|---|
| `context/board/zombies/` | `ZMB` | BO1 zombies port, BO1 content and assets |
| `context/board/skate/` | `SK8` | skate movement, tricks, camera, pad input |
| `context/board/menu/` | `MENU` | mod menu, mod guide, console commands |
| `context/board/iw4/` | `IW4` | engine, tooling, splitscreen, cross-area quality |

A card goes on the board of the code it changes most; a new area gets a new board (copy a config, pick a prefix, keep the same columns). `depends_on` works inside one board; a dependency on another board's card is a line in `## Notes`.

Columns (every board): `idea` → `new` → `doing` → `testing` → `finished`. `your-move` holds cards waiting on the user.

## Card layout

Every card has these sections, in this order:

- `## Goal`: the problem and the intended behaviour, from the player's point of view.
- `## Acceptance Criteria`: checkboxes. These are the **Spec axis** for `/code-review`.
- `## Spec` (written by `/to-spec`, absent until then): Implementation decisions, Testing decisions (the agreed **seams**), Out of scope. No file paths or code snippets: they go stale.
- `## Notes`: quotes from the user (`> @user dd/mm: …`) and questions for the user (`> @claude dd/mm: FOR YOU: …`).
- `## Progress`: one dated line per session, what was done and verified, linking the artifact under `context/artifacts/` that holds the evidence.

## Mapping from the skills' vocabulary

| Skill says | On this board |
|---|---|
| publish a spec | write the `## Spec` section of the card being worked on; refine `## Acceptance Criteria` |
| publish tickets / child issues | one new card per ticket via the `ordna` skill, in column `new`, `depends_on:` set to the blocking cards; the parent card lists the children in `## Notes` |
| fetch the ticket | read `context/board/<area>/tasks/<ID>.md` (the prefix names the area) |
| `ready-for-agent` | column `new` |
| `needs-info` / waiting on a human | column `your-move` |
| claimed / in progress | column `doing` |
| resolved | column `finished` (only after clippy, tests or screenshots verified it); `testing` only when the user must try it in game |
| comments | a `> @user` or `> @claude` line under `## Notes` |

Leftover scope never keeps a card open: split it into a new card. Commits and README bullets ("This fork") name the card they belong to (`ZMB-035`, `SK8-047`…).
