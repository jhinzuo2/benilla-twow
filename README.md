# benilla-TWOW — Turtle WoW fork of benilla (with iNilla for iOS)

This repository is a fork of [benilla](https://github.com/samwhosung/benilla), the from-scratch
World of Warcraft 1.12.1 client in Rust and Bevy, extended for **Turtle WoW** and for **mobile**
(Android, and iOS as **iNilla**). Everything specific to this fork is in this first part; the
upstream benilla README follows below the separator, unchanged in substance.

## Downloads

Every build of `main` and every pull request produces artifacts in the **Actions** tab. Tagged
builds (`v*`) are published as one GitHub Release holding every platform:

| Platform | Asset | Notes |
|---|---|---|
| Windows x86_64 | `benilla-windows-x86_64.exe` | dev build (debug panel, perf HUD) |
| Linux x86_64 | `benilla-linux-x86_64` | dev build; `chmod +x` after download |
| macOS arm64 | `benilla-macos-aarch64` | dev build; `chmod +x` after download |
| all desktop | `benilla-perf-*` | player build (`--profile ship`, no dev instruments) — the one to benchmark |
| Android arm64 | `benilla-android-aarch64.apk` | signed release APK |
| iOS arm64 | `iNilla-<version>.ipa` | iOS 17+, ad-hoc signed — sideload with SideStore, AltStore or LiveContainer |

`SHA256SUMS.txt` in each release lists the checksum of every asset.

## TurtleWoW compatibility

benilla already speaks the stock 1.12.1 protocol, which gets you onto a TurtleWoW realm on its
own — this section is the gap-filling on top of that, specific to what TurtleWoW's own client
patch changed or added.

- **Two extra playable races** — Goblin (Horde) and Blood Elf / High Elf (Alliance). Character
  creation runs TurtleWoW's own GlueXML off your install, the create grid and the race names
  come from its DBCs, and the faction lookup places both races on their side.
- **Sound kits TurtleWoW references that the vanilla DBC doesn't ship** no longer error-spam — a
  missing kit id is warned once, then plays as silence, same as a kit with no attached file.
- **Everlook's internet-radio towers are recognized** — their `SoundEntries` path is a stream
  URL, not an MPQ path — and no longer error every 5 seconds. Actually streaming the station
  isn't implemented yet: tuning in correctly silences the zone music but plays nothing, tracked
  in [#3](https://github.com/jhinzuo2/benilla-twow/issues/3).
- **Camera Lua API extended** with `CameraZoomIn` / `CameraZoomOut` (used by things like the
  barbershop's chair camera) and `FlipCameraYaw`.
- **TurtleWoW's interface loads as shipped**: the stock UI runs off your install's own
  `FrameXML.toc`, so TurtleWoW's `json.lua` codec and its exclusive FrameXML files load in its own
  order.
- **Realm login on TurtleWoW-derived realmd**: the logon presents build 7272 to realmd (which
  rejects 5875) and 5875 to the world server, and a challenge-shaped version reject is reported
  as such instead of a protocol error.
- **Custom maps and missing assets don't hang the loading screen**: a map id with no `Map.dbc`
  row, or a placement whose model is missing from the data, is skipped as the reference client
  skips it.
- **Options → Graphics no longer errors** on a TurtleWoW install — its patch replaces
  `OptionsFrame.lua` with its own category/search system, which doesn't define the
  slider-bounds table the panel reads, so those bounds are now vendored as a fallback.
- **Fixed a client-breaking Lua gap:** `getfenv`/`setfenv` weren't implemented at all, and
  TurtleWoW's own `Globals.lua` opens with `_G = getfenv(0)` — so the whole file, and everything
  chained after it (`wipe`, `trim`, `explode`, `sizeof`, `print`), silently failed to load. That
  surfaced as unrelated-looking nil-global errors deep inside `UIParent.lua`, nowhere near the
  real cause. Both are implemented now.
- **Warden is not implemented** — no crypto, no module execution, and that isn't planned to
  change — but the client no longer disconnects over receiving `SMSG_WARDEN_DATA`, since
  TurtleWoW appears to send it without enforcing it. This is a "don't treat an unenforced packet
  as fatal" decision, not Warden support: a server that does enforce it will still silently kick
  you after about 30 seconds rather than giving a clear error.

Known open gaps, tracked as issues: TransmogUI renders only its background
([#2](https://github.com/jhinzuo2/benilla-twow/issues/2)), the Inspect window's Talents tab is
empty pending FrameXML work ([#7](https://github.com/jhinzuo2/benilla-twow/issues/7)), and some
Tauren gear/hair combinations render white under a cause not yet identified
([#6](https://github.com/jhinzuo2/benilla-twow/issues/6)).

## Platforms

Desktop, Android and iOS build from the same source tree. Status below is what has actually been
run and fixed against on real hardware, not just what compiles in CI.

- **Windows** — actively developed against. `benilla-config` resolves to the running user's
  Documents folder through the Windows shell API (`SHGetKnownFolderPath`), rather than assuming
  an XDG-style path the way Linux does. Builds in CI (`windows-latest`) and has been run for real.
- **Android** — the primary mobile target, and the platform with the most fixes behind it so
  far: the missing `INTERNET` permission (nothing connects without it — `cargo-apk` has no
  manifest-merging step, so the permission list in `benilla-android/Cargo.toml` is the *entire*
  source of truth for the APK's manifest) is fixed, `WOW_DATA` defaults to the app's own media
  directory, a Bevy default-feature conflict between `game-activity` and the `native-activity`
  this project actually targets is resolved, and a signed release APK builds from CI.
  Compile-and-link and signed packaging are proven; nobody has confirmed a full play session on
  a device or emulator yet, so treat it as "should work," not "confirmed."
- **iOS (iNilla)** — the benilla client for iPhone and iPad, with touch, game-controller support
  and an in-app launcher (`crates/inilla-*`, `crates/benilla-app/src/inilla/`, `ios/`). The
  `.ipa` targets iOS 17 or later on arm64 and is ad-hoc signed: SideStore, AltStore and
  LiveContainer re-sign it on install. On first launch it asks for the game data — your own
  1.12.1 `Data` folder, copied in or downloaded from your server's data host. A crash or black
  screen leaves the session log under Files → iNilla → benilla-config → Diagnostics.
- **Linux (Ubuntu)** — builds in CI (`ubuntu-latest`) the same as upstream, and is presumed fine
  since it's the primary Bevy development target. Not independently verified by this fork beyond
  that CI build.
- **macOS** — builds in CI (`macos-latest`, Apple silicon). No macOS-specific work has happened
  in this fork — genuinely unverified; nobody here has run it.

### Touch input (Android)

- An on-screen virtual joystick, bottom-left, floating — it centers on wherever your thumb lands
  rather than sitting at a fixed spot — mapped to WASD-equivalent movement.
- Free-swipe camera look anywhere else on the screen.
- Every finger is classified once, on touchdown — joystick, UI, or world-look — and keeps that
  role until it lifts. That structure is what fixed an earlier bug where a tap's release could be
  silently dropped (the real cause of "taps need spamming" and "a second resting finger fixes
  it"), rather than patching around it case by case.
- **Status:** implemented and merged, not yet confirmed on a real device.
- Planned next, tracked in
  [#8](https://github.com/jhinzuo2/benilla-twow/issues/8): face/camera-based target selection
  without needing Tab, and a more mobile-tailored FrameXML layout with auto-login.

## Setup (this fork)

You need your own Turtle WoW (or other 1.12.1) client for the game data; benilla only reads it
and never writes into it. Point `WOW_DATA` at its `Data` folder (see "Running it" below), and
`WOW_HOST` at the realm.

Where local state lives:

- **Windows:** `C:\Users\USERNAME\Documents\benilla-twow\benilla-config`
- **Android:** `/sdcard/Android/media/benilla-config`
- **iOS:** Files → iNilla → `benilla-config`
- **Linux / macOS dev build:** `benilla-config/` at the repo root

Font support: put the five original font files in `benilla-config/fonts/` — `ARIALN.ttf`,
`FRITZQT__.ttf`, `skurri.ttf`, `MORPHEUS.ttf`, `WarSansTT-Bliz-500.ttf`.

## Building (this fork)

Desktop builds work as upstream describes below (`cargo run --release -p benilla`). The other
targets:

- **Android:** `cargo apk build --release -p benilla-android` with the Android SDK
  (`platforms;android-31`, `build-tools;34.0.0`), NDK r26d and the `aarch64-linux-android` Rust
  target. A release APK is signed through `CARGO_APK_RELEASE_KEYSTORE` and
  `CARGO_APK_RELEASE_KEYSTORE_PASSWORD`.
- **iOS:** `ios/build-staticlib.sh` builds `libinilla_ios.a` (it runs on Linux), then
  `ios/package-ipa.sh` links, bundles and signs `iNilla.ipa` on a Mac with Xcode. The iOS build
  type-checks on Linux without an SDK:
  `source ios/toolchain/linux-env.sh && cargo check --target aarch64-apple-ios -p inilla-ios`.
- **iNilla's own checks:** `cargo test -p inilla-core -p inilla-platform` and
  `cargo check -p benilla-app`.

### CI and releases

| Workflow | When | What |
|---|---|---|
| `build.yml` | push to `main`, pull requests, `v*` tags, manual | builds every platform in parallel — Windows, Linux, macOS (dev and perf builds), Android, iOS — and uploads each as an artifact; a `release` job bundles them into one GitHub Release |
| `ci.yml` | pull requests from forks, manual | upstream's install-free gates (fmt, clippy, tests, lints) |
| `inilla-check.yml` | every push and pull request | iNilla's fast Linux gate: engine-free tests, desktop and iOS type-checks |
| `trace.yml` | manual | Windows player build with `trace_chrome` for profiling |
| `lockfile.yml` | manual | regenerates and commits `Cargo.lock` after a dependency change |
| `inilla-budget.yml` | weekly, manual | reports Actions minutes against the private repository's quota |

The iOS jobs spend macOS minutes, so on a plain push or pull request they are skipped; they run
on tags and manual runs. To publish a release, push an annotated tag `v1.2.3` (its message
becomes the release text), or run `build.yml` from the Actions tab with `release` set to the tag
and `notes` filled in. A manual run with `release` empty is a test build: artifacts only. Releases
are created as pre-releases until a build is marked tested by hand.

---

<div align="center">
  <h1>benilla</h1>
  <p><b>A complete World of Warcraft 1.12.1 client, written from scratch in Rust and <a href="https://bevy.org">Bevy</a></b></p>
  <p>
    <a href="https://discord.gg/wJSJx467G4"><img src="https://img.shields.io/discord/1529280129518538922?style=for-the-badge&logo=discord&logoColor=white&label=discord&color=5865F2" alt="Discord"></a>
    <a href="https://www.youtube.com/playlist?list=PLdCnpZNKxyb8"><img src="https://img.shields.io/badge/devlog-youtube-FF0000?style=for-the-badge&logo=youtube&logoColor=white" alt="YouTube devlog"></a>
    <a href="LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue?style=for-the-badge" alt="License"></a>
  </p>
</div>

benilla plays the whole game: character creation, questing and professions, dungeons and raids,
battlegrounds and honor, groups, guilds, trade, mail and the auction house, on the stock 1.12
interface and with your addons. It connects to a 1.12.1 server over the original protocol and
reads the game's data from your own 1.12.1 install. Every file format, the network protocol and
the interface engine are written from scratch, with no original client code, no third-party WoW
crates and no bundled game assets.

What the real client does is read at its bytes in
[benilla-re](https://github.com/samwhosung/benilla-re), the reverse engineering benilla is built
against, by the same client addresses the comments here cite.

## What's inside

- **Formats:** readers for the whole asset stack (the MPQ patch chain, BLP, DBC, ADT, WDT, WDL, M2
  and WMO), wired into Bevy as an asset source.
- **World:** terrain streamed out to the horizon, portal-culled buildings with interior lighting,
  doodads and ground clutter, swimmable water, sky and weather, and the client's own day/night
  lighting, fog and gamma.
- **Models:** GPU-skinned M2s with the full animation controller, particles, ribbons and the spell
  visuals, and characters end to end: customization, the armor composite, weapons with their
  enchant glows, forms, stealth and mounts.
- **Movement:** networked movement in both directions, the server-granted modes from slow fall to
  roots, a follow camera with collision, boats, zeppelins and taxi flights.
- **Networking:** SRP6 auth through world-session crypto and the object mirror into the ECS,
  covering the game from movement and chat through combat, spells, groups and raids, quests,
  trade, mail, the auction house and battlegrounds.
- **Interface:** a FrameXML and Lua engine that runs the stock 1.12 interface off your install's
  patch chain, and third-party 1.12 addons: Questie, pfUI, Bagnon, Bartender2 and most others
  run. By choice, the options window and the ESC menu follow the Classic Era client's rather than
  1.12's.
- **Audio:** music, ambience and effects under the client's own selection and crossfade rules,
  with interior and underwater transitions and zone reverb.

The format readers and the UI engine core are plain Rust with no Bevy in them, and the world
renderer runs with no game attached. [`docs/MAP.md`](docs/MAP.md) maps every crate and subsystem,
and [`docs/DEVIATIONS.md`](docs/DEVIATIONS.md) lists everywhere benilla differs from 1.12.1; both are
generated from the code.

## Status

Complete and fully playable. What is left:

- The long tail of small features that separates a working client from a finished one, tracked
  as [issues](https://github.com/samwhosung/benilla/issues).
- Addons, options and performance, ongoing.

benilla is a faithful 1.12.1 client and the foundation people build on. There are no prebuilt
downloads: a packaged build, a particular server's changes or anything 1.12.1 never had belongs
in a fork, and forks are welcome. GitHub lists
[every public fork](https://github.com/samwhosung/benilla/forks).

Not planned: other expansions or client versions, Warden (anticheat).

## Running it

benilla builds and runs on macOS, Linux and Windows. You need:

- **A 1.12.1 client (build 5875)** for the game data, in any locale. benilla only reads it, and
  shows its text in the client's language; development runs on English clients.
- **A 1.12.1 server.** [vmangos](https://github.com/vmangos/core) is what development runs
  against; cMaNGOS and the other 1.12.1 cores speak the same protocol. benilla does not answer
  Warden, so a server that enforces it disconnects the session after its timeout; vmangos ships
  with Warden off.
- **Stable Rust and a C compiler**, which some dependencies build with: on macOS the
  Xcode command line tools, on Linux the ALSA and udev development packages and pkg-config, on
  Windows the MSVC build tools that the Rust installer sets up.

```sh
WOW_DATA=/path/to/WoW/Data cargo run --release -p benilla
```

On Windows, in PowerShell:

```powershell
$env:WOW_DATA="C:\path\to\WoW\Data"; cargo run --release -p benilla
```

Each release is a tag on the [Releases page](https://github.com/samwhosung/benilla/releases):
`git checkout <tag>` first runs that release, and `main` is the development tip.

`WOW_DATA` names the install's `Data` folder; a link to the install named `WoW` at the repo root
does the same (`ln -s /path/to/WoW WoW`, or on Windows a junction, which needs no admin rights:
`New-Item -ItemType Junction -Path WoW -Target C:\path\to\WoW`). The server defaults to
`localhost:3724`, the stock auth port. Point `WOW_HOST` at another (`WOW_HOST=play.example.com`, or
`play.example.com:5000` for a remapped port), or set it from the Realmlist button on the login
screen, which remembers it. Credentials go in at the login screen, or set `WOW_USER` and `WOW_PASS`
to skip it.

Settings, screenshots and addons live in `benilla-config/` at the repo root: a 1.12 addon goes in
`benilla-config/AddOns/`. [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md) has the rest, from the
player build to the tests.

## Contributing

Issues and pull requests are open. [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md) says where to
start, what gets in, how a change is judged and what happens to a pull request once it is open.
Bugs, questions and ideas are welcome on the [Discord](https://discord.gg/wJSJx467G4) too.

## Credits and license

Early inspiration and file format guidance came from the
[wowemulation-dev](https://github.com/wowemulation-dev) community, and
[warcraft-rs](https://github.com/wowemulation-dev/warcraft-rs) in particular.

benilla is an independent fan project, not affiliated with or endorsed by Blizzard Entertainment.
It ships **no Blizzard content**: no art, models, sounds, maps, MPQ contents or FrameXML. You
provide your own legally obtained 1.12.1 client, and the stock interface runs off its FrameXML at
runtime. The few files under `crates/benilla-app/assets/ui/` are our own, not copies of it:
adapters over stock files, and the settings windows and script error log benilla draws itself.

World of Warcraft is a trademark of Blizzard Entertainment, Inc. Our own code is licensed under
[MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your option. The kira audio engine under
`third_party/` is vendored under the same two licenses, with its author's copyright notices beside it.
