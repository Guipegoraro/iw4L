# IW4L

<p align="center">
  <img src="docs/screenshots/bomb-plant.jpg" width="49%">
  <img src="docs/screenshots/tanker-explosion.jpg" width="49%">
</p>

IW4L is a Call of Duty runtime written from scratch in Rust, on
[bevy](https://bevyengine.org/) and [wgpu](https://wgpu.rs/). Point it at a copy
of MW2 you already own and it loads that install's data into its own engine.

The on-disk layouts came out of reverse engineering the original binaries and
reading public technical references.

This whole project is written by an LLM.

## Status

No releases yet: you build it from this tree. Nothing here is stable either —
the API, the config format, caches and the wire protocol all change between
commits, so everyone in a session needs to be on the same one. Expect bugs and
desyncs.

The network side is for arranged playtests among people who already agreed to
play; it has never been vetted for lobbies full of strangers. IW4L sends nothing
home, and diagnostic files sit on your disk until you attach them to a report.

## This fork

Changes on top of upstream, newest last. Every change is recorded here.

- **Remote control (BRP).** The `brp` launcher feature (on by default) serves
  the Bevy Remote Protocol on `127.0.0.1:15702`: query and change the live
  game, take screenshots, send keys. `BRP_EXTRAS_PORT` moves it.
- **Mod menu.** In a match, **F5** opens it and pauses, **F6** opens it with
  the game running; **F2** works as Esc. Pages: match, player, skate, weapons
  (every giveable gun), bots, visuals, capture, maps. Each entry runs a
  console command, so nothing has to be typed. Console: `modmenu
  [pause|live|close]`. Other code adds pages as data with
  `app.add_mod_menu_page(..)` (`crates/ui/src/mod_menu.rs`).
- **`heal`** restores the local player's health (cheats; on in local matches).
- **`god [on|off]`** refuses all damage to you (mod menu: Player). Cheats are
  per-player bits in `sim::cheat` switched by one `SetCheat` action; a new
  cheat is a bit, the check where it bites, and a console name.
- **`timescale [0.05..4]`** slows or speeds the whole game (1 = normal; mod
  menu: Match has 0.25x, 0.5x, 1x, 2x). It scales the clock that drives the
  fixed tick, so every tick stays identical; only the match host can use it.
- **`gravity [n]` / `speed [n]`** (also `g_gravity` / `g_speed`) set the
  match's gravity (default 800) and move speed (190) for every player, live
  and for later spawns (`crates/sim/src/tuning.rs`). Mod menu: World page,
  with presets.
- **`savepos [slot]` / `loadpos [slot]`** remember where you stand and look
  and teleport back (through `move`), for retrying a line (mod menu: Player).
- **`noclip [on|off]`** flies through walls along the view: jump rises,
  crouch sinks, sprint doubles the speed (mod menu: Player). It is a movement
  mode like skate (`crates/movement_iw4/src/noclip.rs`).
- **Skateboard mode.** `skate [on|off]` (or `movemode <normal|skate>`, or the
  mod menu's Skate page). W pushes, S brakes, A/D carve, holding jump winds up
  an ollie that pops on release, A/D in the air spins the board. A landing
  across the board, a hard impact or a wall bails. The view stays free.
  `showpos on` shows ground speed and the board state. Movement modes are a
  hook in `pm_move` (`crates/movement_iw4/src/mode.rs`); skate lives in
  `skate.rs` with its knobs in `SkateTuning`. Its physics tests:
  `cargo test -p approved_tests` (`crates/approved_tests/src/skate_physics.rs`).
- **Gamepad.** XInput pads (an 8BitDo in XInput mode shows up as an Xbox 360
  controller). Left stick moves (analog: carve depth when skating), right
  stick looks. Buttons are ordinary binds (`bind BUTTON_A +gostand`), console
  MW2 layout by default:

  | button | action | button | action |
  | --- | --- | --- | --- |
  | A | jump / ollie | RT | fire |
  | B | crouch | LT | aim |
  | X | use / reload | RB | lethal |
  | Y | switch weapon | LB | tactical |
  | L3 | sprint | R3 | melee |
  | D-pad | action slots 1–4 | Start / Back | mod menu / pause menu |

  In a menu the D-pad moves, A picks and B goes back. Settings files saved
  before gamepad support get these binds once on load.
- **`thirdperson [on|off]`** (also in the mod menu's Skate and Visuals pages,
  saved with the settings) moves the camera behind the player with the
  retail third-person camera, your own body drawn. Like retail, the camera
  stops at walls and the map's invisible edges, so with one right behind you
  it sits close and your body can drop below the frame. While skating in
  third person the camera is Skate 3's low chase shot behind the board
  (about 2 m back, swinging round with the board); `skatecam
  [chase|shoulder]` (mod menu: Skate) switches to the MW2 shoulder view.
- **Mod guide.** The pause menu (Esc) has a **Mod Guide** entry: how to use
  every addition, one page per topic. A feature adds its own page with
  `app.add_mod_guide_topic(..)` (`crates/ui/src/mod_guide.rs`).
- **Solo matches start at once.** Alone in a match, there is no wait for
  players and no countdown: you play as soon as you spawn.
- **`IW4L_TIME_LIMIT`** sets the match time limit in minutes; `0` plays with
  no limit (e.g. `IW4L_TIME_LIMIT=0` in `.env`). `IW4L_SCORE_LIMIT` already
  existed.
- **Flick-it tricks.** While skating, the pad's right stick does tricks the
  Skate 3 way (holding LT gives it back for aiming): flick down-then-up to
  ollie, and the other Skate 3 gestures for kickflip, heelflip, shuvits,
  varials, hardflip, inward heel, 360 flip, laser flip and their 360 and
  nollie versions. A faster flick pops higher. The board must come round
  before you land or you bail; a half shuv lands you switch. The gestures
  come from Skate 3's own `skater.pat`: set `IW4L_SKATE3_DATA` in `.env` to
  the folder holding `data/joystick/skater.pat` (extracted from your disc).
  Without a pad, `trick <name> [strength]` fires one. Recogniser:
  `crates/skate_input`; trick table: `crates/movement_iw4/src/skate_tricks.rs`;
  the trick rides in `UserCmd::skate_trick` so the sim stays deterministic.
  With a pad, while skating and not aiming, the view swings round to follow
  the board, since the right stick is busy with tricks.
  The board is not drawn yet, so tricks show only as pops and catches.
- **Black Ops 1 content.** Put a shortcut named e.g. `Black Ops.lnk` to a BO1
  install inside the MW2 folder (`IW4L_GAMES`): shortcut targets are extra
  search roots. Then `map t5:mp_nuked` (any `t5:mp_*` map) loads a BO1
  multiplayer map and `give t5:famas`, `give t5:commando`, … hand out BO1
  guns, also on MW2 maps. The zombie maps (`zombie_*`) do not load yet: they
  are single-player zones with other asset layouts (their clipmap has no
  brush table in IW4L's reader) and their gameplay is GSC script, which IW4L
  does not run.
- **Windows test runs.** `cargo run -p approved_tests -- heavy_gameplay_lifecycle`
  works on Windows (junction for the shared cache, `iw4l.exe`, newest log when
  there is no `latest.log`). `IW4L_KEEP_CWD=1` stops the launcher moving into
  its own folder.

### Credits for this fork

- **skate-3-rust-engine** (`SK8-ENGINE/skate-3-rust-engine`, a Rust/Bevy
  Skate 3 reimplementation). `crates/skate_input` is ported from its
  `crates/skate-core/src/input/gesture.rs` (the flick-it pattern recogniser:
  per-point tolerances, miss culling, wind-up hold, scoring and flick
  strength) and `crates/skate-data/src/gesture_patterns.rs` (the PAT gesture
  file parser), with the decompiler address notes removed. Its survey of Skate
  3's pop height, chase camera shots and animation pipeline shapes the skate
  cards still to come. Gesture files, animations and models come from the
  player's own Skate 3 disc and are never committed here.

## Architecture

| | |
|---|---|
| assets | MW2 zones read natively; MW3 and Black Ops land in the same `asset_iw4` IR. |
| shaders | Retail D3D9 SM3 tokens translated to WGSL, so no DirectX at runtime. |
| rendering | One sorted drawsurf list; only the tess emitters fork per surface type. |
| physics | Fixed 17 ms step on its own accumulator. Framerate changes nothing about how a body falls. |
| simulation | One `TickInput → sim::step → Snapshot` funnel for server, prediction and replay. |
| network | Custom p2p wire over UDP: deltas, reliability, reconciliation. A QUIC master only introduces peers. |
| platforms | Linux, macOS (Metal) and a portable Windows build. |

Retail protocols, the original ABI and patched executables are out of scope.
IW4L clients talk to IW4L clients.

<p align="center">
  <img src="docs/screenshots/terminal-sniper.jpg" width="49%">
  <img src="docs/screenshots/jungle-crossbow.jpg" width="49%">
</p>

## Game data

`IW4L_GAMES` points at the folder holding your game trees. No assets ship in
this repository or in any release, and IW4L is unaffiliated with the rights
holders of the original games.

Those trees are read only. IW4L never patches them, swaps files in them or
writes anything back; caches, settings, demos and logs land in
`iw4l-artifacts/` next to the IW4L binary.

## Build and run

System packages first: [`docs/BUILD.md`](docs/BUILD.md) (Fedora / Debian /
Arch — compiler, ALSA, udev, X11/Wayland headers; macOS — Xcode command line
tools).

```bash
cp .env.example .env          # IW4L_GAMES — folder containing the game trees
make map mp_boneyard          # run a map
make map mp_boneyard CMDS='spawn assault; wait 2s; quit'
make help                     # every recipe
```

Live runs use `[profile.play]`, a development build with optimizations turned
on. `PROFILE=release` builds the real release binary.
[`docs/WINDOWS.md`](docs/WINDOWS.md) covers Windows.

## Documentation

Implementation notes live under `docs/`, one short file per area. Start at
[`docs/INDEX.md`](docs/INDEX.md).

| file | about |
| ---- | ----- |
| [`docs/BUILD.md`](docs/BUILD.md)         | system packages per distro, macOS, Windows cross prerequisites |
| [`docs/RUN.md`](docs/RUN.md)           | running the game, console scripts, commands and traps |
| [`docs/PERF.md`](docs/PERF.md)         | Perfetto tracing and performance analysis             |
| [`docs/RENDER.md`](docs/RENDER.md)     | rendering pipeline                                    |
| [`docs/MAP-LOAD.md`](docs/MAP-LOAD.md) | map loading and asset installation                    |
| [`docs/ENTITIES.md`](docs/ENTITIES.md) | simulation data flow and entity taxonomy              |
| [`docs/SIM-STEP.md`](docs/SIM-STEP.md) | simulation step architecture                          |
| [`docs/ANIM.md`](docs/ANIM.md)         | animation system                                      |
| [`docs/WINDOWS.md`](docs/WINDOWS.md)   | portable Windows build                                |
| [`docs/DEPLOY.md`](docs/DEPLOY.md)     | release, publishing and deployment                    |
| [`docs/MASTER.md`](docs/MASTER.md)     | running your own master server                        |

## Contributing and support

A personal, experimental project. Bug reports are welcome and get no promised
fix date. [`CONTRIBUTING.md`](CONTRIBUTING.md) says what a useful report
contains and how changes get reviewed; security reports go to
[`SECURITY.md`](SECURITY.md).

## Acknowledgements

IW4L ships none of the code below. It was read against all of it.

* [OpenAssetTools](https://github.com/Laupetin/OpenAssetTools) and its
  [iw4x-x64 fork](https://github.com/iw4x-x64/oat) — modding tools whose
  asset-structure headers document the on-disk layouts IW4L reads.
* [IW4x](https://github.com/iw4x/iw4x-client) — a custom client for MW2 (2009),
  a cross-reference for asset and protocol behaviour.
* [KisakCOD](https://github.com/SwagSoftware/KisakCOD) — an open-source CoD4
  reimplementation, a cross-reference for engine structure one generation over
  in the same family.
* [Ghidra](https://github.com/NationalSecurityAgency/ghidra) — the framework the
  original binaries were read with.

<p align="center">
  <img src="docs/screenshots/industrial-daylight.jpg" width="49%">
  <img src="docs/screenshots/domination-capture.jpg" width="49%">
</p>

## License

IW4L is licensed under the [Apache License 2.0](LICENSE), and
[`NOTICE`](NOTICE) holds the copyright notices, the licences of the projects
above and the bundled fonts. That licence covers IW4L's own source code. Call
of Duty, Modern Warfare, Black Ops and the related assets, trademarks and
intellectual property belong to their respective owners.
