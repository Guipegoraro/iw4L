# Issue tracker: the ordna board

Issues for this fork are **cards** on the ordna board: one Markdown file per card in `context/board/tasks/` (git-ignored working memory; `.ordna/config.yaml` holds the columns and the id prefix `MOD`). The `ordna` skill reads, creates and moves cards; use it instead of editing frontmatter by hand.

Columns: `idea` → `new` → `doing` → `testing` → `finished`. `your-move` holds cards waiting on the user.

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
| fetch the ticket | read `context/board/tasks/<ID>.md` |
| `ready-for-agent` | column `new` |
| `needs-info` / waiting on a human | column `your-move` |
| claimed / in progress | column `doing` |
| resolved | column `finished` (only after clippy, tests or screenshots verified it); `testing` only when the user must try it in game |
| comments | a `> @user` or `> @claude` line under `## Notes` |

Leftover scope never keeps a card open: split it into a new card. Every commit names the card id (`MOD-nnn`) it belongs to, and the README bullet ("This fork") for the feature names it too.
