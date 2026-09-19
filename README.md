<div align="center">
  <h1>benilla</h1>
  <p><b>A from-scratch World of Warcraft 1.12.1 client in Rust and <a href="https://bevy.org">Bevy</a></b></p>
  <p>
    <a href="https://discord.gg/wJSJx467G4"><img src="https://img.shields.io/discord/1529280129518538922?style=for-the-badge&logo=discord&logoColor=white&label=discord&color=5865F2" alt="Discord"></a>
    <a href="https://www.youtube.com/playlist?list=PLdCnpZNKxyb8"><img src="https://img.shields.io/badge/devlog-youtube-FF0000?style=for-the-badge&logo=youtube&logoColor=white" alt="YouTube devlog"></a>
    <a href="LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue?style=for-the-badge" alt="License"></a>
  </p>
</div>

> **Issues and pull requests are closed upstream at @samwhosung/benilla here in my fork (@jhinzuo2/benilla-twow) I'd love some Issues posted or PR's incase you found something i missed regarding turtle-wow, windows build or the Android one.** 
> 
>
>The main way to contribute for anything regarding benilla and not benilla-twow(my Fork) should join the [Discord](https://discord.gg/wJSJx467G4) and report
> the bugs you find. Questions and ideas are welcome in the same place.

benilla speaks the original 1.12.1 protocol, so it connects to any server the real client could,
and reads its game data at runtime from your own 1.12.1 install. Every file format and the network
protocol are implemented from scratch, with no original client code, no third-party WoW crates,
and no bundled game assets.

## What works

- **Formats:** readers for the full asset stack (MPQ patch chain, BLP, DBC, ADT/WDT/WDL, M2, WMO),
  wired into Bevy as an asset source.
- **World:** streamed terrain out to the horizon, portal-culled WMOs with interior lighting,
  doodads and ground clutter, swimmable liquids, sky and weather, and the client's own day/night
  lighting, fog and gamma passes.
- **Models:** GPU-skinned M2s with the full animation controller, a near feature-complete particle
  system, ribbons, and animated gameobjects from doors to lifts.
- **Characters:** customization end to end, the armor texture composite, weapons with sheathing and
  enchant glows, shapeshift forms, stealth and mounts.
- **Movement:** a WoW-feel controller, networked movement in both directions, the server-granted
  modes from slow fall to roots, a follow camera with collision, boats, zeppelins and taxi flights.
- **Networking:** SRP6 auth through world-session crypto, the object mirror into the ECS, and live
  wire coverage from movement and chat through spells, party, quests, mail, trade, vendors, bank,
  loot, the auction house and PvP honor.
- **UI:** a from-scratch FrameXML + Lua engine driving the built-in interface, from the login and
  character screens through the full HUD, the classic windows (guild, macros and key bindings
  included), chat, nameplates, floating combat text and tooltips; third-party addons load from
  a `benilla-config/AddOns/` folder beside the executable (partial: AtlasLoot and Bagnon run,
  see below).
- **Combat:** melee on the faithful swing law, ranged and Auto Shot, casting with GCD and
  cooldowns, combo points, crowd control that really holds you, and the spell visual pipeline.
- **Audio:** music, ambience and SFX under the client's own selection and crossfade rules, with
  interior and underwater transitions and zone reverb.

## Platforms

Desktop and Android build from the same source tree. Status below is what has actually been run
and fixed against on real hardware, not just what compiles in CI.

- **Windows** — actively developed against. `benilla-config` now resolves to the running user's
  Documents folder through the Windows shell API (`SHGetKnownFolderPath`), rather than assuming
  an XDG-style path the way Linux does, including a follow-up fix for a Windows API type
  mismatch the first pass got wrong (a null handle, not a zero integer). Builds in CI
  (`windows-latest`) and has been run for real.
- **Android** — the primary mobile target, and the platform with the most fixes behind it so
  far: the missing `INTERNET` permission (nothing connects without it — `cargo-apk` has no
  manifest-merging step, so the permission list in `benilla-android/Cargo.toml` is the *entire*
  source of truth for the APK's manifest) is fixed, `WOW_DATA` defaults to the app's own media
  directory, a Bevy default-feature conflict between `game-activity` and the `native-activity`
  this project actually targets is resolved, and a signed release APK builds from CI. See
  **Touch input** below for the control scheme. Compile-and-link and signed packaging are
  proven; nobody has confirmed a full play session on a device or emulator yet, so treat it as
  "should work," not "confirmed."
- **Linux (Ubuntu)** — builds in CI (`ubuntu-latest`) the same as upstream, and is presumed fine
  since it's the primary Bevy development target — no Linux-specific bugs have come up because
  none have needed fixing. Not independently verified by this fork beyond that CI build.
- **macOS** — builds in CI (`macos-latest`). No macOS-specific work has happened in this fork at
  all — genuinely unverified; nobody here has run it.

### Touch input (Android)

- An on-screen virtual joystick, bottom-left, floating — it centers on wherever your thumb lands
  rather than sitting at a fixed spot — mapped to WASD-equivalent movement.
- Free-swipe camera look anywhere else on the screen.
- Every finger is classified once, on touchdown — joystick, UI, or world-look — and keeps that
  role until it lifts. That structure is what fixed an earlier bug where a tap's release could be
  silently dropped (the real cause of "taps need spamming" and "a second resting finger fixes
  it"), rather than patching around it case by case.
- **Status:** implemented and merged, not yet confirmed on a real device. CI proves the APK
  compiles, packages, and — as of the last signed build — that release signing works; an actual
  play session hasn't happened yet.
- Planned next, tracked in
  [#8](https://github.com/jhinzuo2/benilla-twow/issues/8): face/camera-based target selection
  without needing Tab, and a more mobile-tailored FrameXML layout with auto-login.

## TurtleWoW compatibility

benilla already speaks the stock 1.12.1 protocol, which gets you onto a TurtleWoW realm on its
own — this section is the gap-filling on top of that, specific to what TurtleWoW's own client
patch changed or added.

- **Two extra playable races** — Goblin (Horde) and Blood Elf / High Elf (Alliance) — through
  character creation, the race-select icon art (read from the client's own
  `CharacterCreate.lua` icon table when present, the frozen vanilla math otherwise), and the
  per-race equipment/helm model variants.
- **Sound kits TurtleWoW references that the vanilla DBC doesn't ship** no longer error-spam — a
  missing kit id is warned once, then plays as silence, same as a kit with no attached file.
- **Everlook's internet-radio towers are recognized** — their `SoundEntries` path is a stream
  URL, not an MPQ path — and no longer error every 5 seconds. Actually streaming the station
  isn't implemented yet: tuning in correctly silences the zone music but plays nothing, tracked
  in [#3](https://github.com/jhinzuo2/benilla-twow/issues/3).
- **Camera Lua API extended** with `CameraZoomIn` / `CameraZoomOut` (used by things like the
  barbershop's chair camera) and `FlipCameraYaw`.
- **`benilla.toc` updated** to load TurtleWoW's own `json.lua` codec and the block of
  TurtleWoW-exclusive FrameXML files its manifest appends, in TurtleWoW's own load order; the
  two real functionality gaps that block on (`StopMusic`, `CameraZoomOut`) are filled by host
  verbs.
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

## Where it's going

benilla is done when a 1.12.1 player can do everything here that they could in the original
client, it looks and feels the same, and it runs from a download on Windows, Linux and macOS.
No dates; the order is what is likely, not a promise.

- Battlegrounds, then the long tail of small features that separates a working client from a
  finished one.
- Addons, options and performance, ongoing.
- The no-brainer fixes from VanillaFixes, SuperWoW and the like.
- Playable downloads for Windows, Linux and macOS. Linux first.

Not planned: other expansions or client versions, Warden (anticheat).

## Running it / Environment Variables

You need a **1.12.1 (build 5875) client install** for game data, a vanilla server to connect to,
and stable Rust. Any 1.12.1 core works; [vmangos](https://github.com/vmangos/core) is what
development runs against, and cMaNGOS and the rest speak the same protocol.

```sh
WOW_DATA=/path/to/WoW/Data cargo run --release -p benilla
```

The server defaults to `localhost:3724`, the stock `realmd` auth port. Point `WOW_HOST`
at any IP or hostname, appending the auth port if yours is remapped
(`WOW_HOST=play.example.com:5000`). Credentials go in at the login screen, or set `WOW_USER` /
`WOW_PASS` to skip it.

Windows config lands in C:\Users\USERNAME\Documents\benilla-twow\benilla-config

Android config lands in /sdcard/Android/media/benilla-config

Font Support via the 5 Original Fontnames inside /benilla-config/fonts. 
(ARIALN.ttf, 
FRITZQT__.ttf, 
skurri.ttf, 
MORPHEUS.ttf, 
WarSansTT-Bliz-500.ttf)

---

Early inspiration and file format guidance came from the
[wowemulation-dev](https://github.com/wowemulation-dev) community, and
[warcraft-rs](https://github.com/wowemulation-dev/warcraft-rs) in particular.

benilla is an independent fan project, not affiliated with or endorsed by Blizzard Entertainment.
It ships **no Blizzard content** — no art, models, sounds, maps, MPQ contents or FrameXML; you
provide your own legally obtained 1.12.1 client. The interface code under
`crates/benilla-app/assets/ui/` is ours, written to the client's own layout and API names so that
the windows look right and 1.12.1 addons find the names they expect.

World of Warcraft is a trademark of Blizzard Entertainment, Inc. Our own code is licensed under
[MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your option.
