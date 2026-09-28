# Proving a change

A card is verified when every claim in its `## Progress` line has a check
behind it that goes **red** when the claim is false. Take the cheapest check
that can go red:

1. **A test in `crates/approved_tests`** for logic and script threads. Drive
   the real code: threads through `gsc_threads::Scheduler`, and the mode's own
   helpers for its engine half (`spawner::zombie_spawn_init_threads`,
   `damage::apply_zombie_damage`) rather than a copy of what the mode does.
   Pin numbers from the original source (GSC, retail), written literally,
   never read back from the constants under test.
2. **A live run** for anything the engine owns: collision, spawning,
   animation, what the player sees. Scripting and verbs: [`../RUN.md`](../RUN.md).
   Logs land in `target/<profile>/iw4l-artifacts/logs/` (newest file),
   screenshots in `…/screenshots/`; look at a screenshot before trusting it.
3. **A probe** when the log does not say it: a temporary
   `diag::info!(Sim, "PROBE …")`, grep the log for `PROBE`, then delete it
   (`grep -rn PROBE crates/<crate>` comes back empty) before the commit.

## Every test goes red once

Before a test counts, break what it guards and watch it fail: copy the file
aside, revert the fix or mutate one constant, run that one test, restore the
copy (`git diff --stat` on the file shows it back). A test that stays green on
the broken code guards nothing; tighten it until it goes red.

## Setting the scene

- **Place the player** with `move x y z` (`tp`); local matches have cheats
  on. Map coordinates come from the map's entity string:
  `iw4l export-rawfiles t5:<zone>` writes it to
  `iw4l-artifacts/rawfiles/<map>/mapents.txt`. A brush-model entity
  (`"model" "*N"`) sits at its own `origin`; the model's bounds are local.
- **Hurt, kill, add bots** with the `damage`, `kill` and `bot` verbs.
- **Time in probes is sim time.** It lags the wall clock by the load, so a
  `wait 30s` in `--cmds` is not 30 000 ms of `now_ms`; gate a periodic probe
  on sim time (`now_ms % 1000 < 50`) and log the time with it.

## A missing lever is a tool

When a check needs something no verb or query gives (a teleport, a spawn, a
readout), build it as a small reusable tool and list it here: a console
command registered like the ones in `crates/console/src/debug_move.rs`, or a
query on `sim::ModeEngine` (as `player_touches_brush_model` is). The next
card reuses it instead of rebuilding a one-off.

Queries built this way: `ModeEngine::{ground_z, actor_clip,
player_touches_brush_model}`. Console verbs are listed in `../RUN.md`.
