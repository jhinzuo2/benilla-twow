# benilla Android scaffold — status

Goal for this pass: get to "opens a window on a real device" — no touch UI, no storage-permission
UX, no addon/FrameXML work. That's it.

## What's here

- `crates/benilla-android/` — drop this whole folder into your fork's `crates/` directory.
  `members = ["crates/*"]` in the workspace root picks it up with no other Cargo.toml edit.
- A one-line addition to `crates/benilla-app/Cargo.toml` (already applied to the clone this was
  built against — re-apply the same `[target.'cfg(target_os = "android")'.dependencies]` block to
  your fork) that turns on bevy_winit's `android-native-activity` feature.

## What this actually does

1. `android_main(app: AndroidApp)` is the entry point the JVM reaches via `cargo-apk`/`cargo-ndk`
   (build tooling — see "not done" below).
2. Sets `WOW_DATA` and `BENILLA_HOME` to `<external-files-dir>/WOWDATA` and
   `<external-files-dir>/benilla-config` — i.e. `Android/data/<package>/files/...`, which needs no
   runtime storage permission and is where you'd drop your 1.12.1 `Data/` folder via any file
   manager. Verified against `local_state.rs` and `benilla-formats/src/install.rs`: both already
   resolve these as their first-priority env var override, so no engine code changes were needed
   for this part.
3. Hands the `AndroidApp` to `bevy::winit::ANDROID_APP` (a `pub static OnceLock<AndroidApp>`),
   which is the real mechanism `WinitPlugin` reads when it builds its `EventLoop` — confirmed
   against bevy_winit's own source, not guessed. No `benilla_app::run()` signature change needed.
4. Calls the same `benilla_app::run()` the desktop shim calls, unmodified.

## What's NOT done — the actual next steps, roughly in order

1. **Build tooling.** Nothing here sets up `cargo-apk` or `cargo-ndk` + a Gradle wrapper, an
   `AndroidManifest.xml`, or app icons/package id. That's the very next thing needed before any
   of this can even compile-and-run — right now this is source with no way to produce an APK yet.
2. **First-run reality check.** wgpu's Vulkan backend on Android, surface creation timing on
   `Resumed` (NativeActivity's surface isn't available until after that lifecycle callback, and
   Bevy/winit's Android backend has had real historical bugs here — see bevyengine/bevy#8874, a
   black-screen issue with a `Gl` backend fallback on one device), and whatever your specific
   device's GPU/driver does that a search engine can't predict. Expect this to be where most of
   the actual debugging time goes, not the Rust code.
3. **Touch input.** Nothing here adds a movement joystick or camera-drag. The existing
   keyboard/mouse-driven movement and camera systems are untouched; a touch source needs to feed
   the same systems, not replace them.
4. **Audio.** `cpal` is already in the dependency graph for non-macOS targets (confirmed in
   `benilla-app/Cargo.toml`), and cpal has an Android/Oboe backend, but this hasn't been tested —
   flagged as "probably fine, unverified" rather than "known working."
5. **Lifecycle correctness.** Backgrounding, `Paused`/`Resumed` cycling, and the network
   connection surviving any of that are explicitly out of scope for this pass — expect a quick
   test session to break if you switch apps mid-session, and don't chase that yet.

## A note on how this was put together

Earlier drafts of this scaffold assumed a standalone `android-activity` dependency and a manual
`EventLoopBuilderExtAndroid::with_android_app()` call — both wrong, corrected after checking
winit's actual docs and bevy_winit's actual source rather than trusting the first plausible-looking
API shape. Worth rebuilding early and reading the real compiler errors rather than assuming the
rest of this is equally solid — it's been checked against real sources where checked, but it has
never been compiled.
