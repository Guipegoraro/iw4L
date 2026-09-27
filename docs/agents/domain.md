# Domain docs

The root `CONTEXT.md` is the **maintainer contract** for `context/` (artifacts, iterations, evidence) and already carries that glossary. The game-domain glossary lives beside the other docs so the two stay distinct:

- `docs/CONTEXT.md`: the domain glossary (engine terms, sim/entity terms, mod terms, skate and zombies terms). Read it, and the root `CONTEXT.md`, before exploring the codebase for a card.
- `docs/adr/NNNN-<slug>.md`: decisions with a load-bearing reason (why a system is built this way, what was rejected), e.g. the GSC-port strategy for BO1 zombies and the skate stick/camera rules.
- `docs/<TOPIC>.md`: the one-page maps in `docs/INDEX.md` (how a system works today). These are notes, not decisions; an ADR records the *why*.

Both new files are created lazily by `/domain-modeling` (reached through `/grill-with-docs` and `/improve-codebase-architecture`) the first time a term or decision is actually resolved; when a skill's text says `CONTEXT.md` it means `docs/CONTEXT.md` here. When they are missing, proceed silently.

Use the glossary's terms in card titles, spec sections, test names, type and module names. A concept you need that the glossary lacks is a signal: either the project has no word for it yet (note it for `/domain-modeling`) or you are inventing a synonym for one it has.

Output that contradicts an ADR says so explicitly: *"Contradicts ADR-0002 (…), worth reopening because …"*.
