//! The Android entry point — a `#[bevy_main] fn main()`, not a hand-written `android_main`.
//!
//! Earlier drafts of this file wrote `android_main(app: AndroidApp)` directly and tried to set
//! `bevy::winit::ANDROID_APP` themselves. Both were wrong, discovered only once this crate
//! actually failed to compile in CI (`unresolved import bevy::winit::AndroidApp` / `cannot find
//! value ANDROID_APP in crate bevy::winit`) and the real `bevy_derive::bevy_main` macro source
//! was checked directly (bevy_derive 0.18.1, `src/bevy_main.rs` — matches this project's pinned
//! bevy version exactly). That source is unambiguous about the actual shape:
//!
//! ```ignore
//! #[unsafe(no_mangle)]
//! #[cfg(target_os = "android")]
//! fn android_main(android_app: bevy::android::android_activity::AndroidApp) {
//!     let _ = bevy::android::ANDROID_APP.set(android_app);
//!     main();
//! }
//! ```
//!
//! `#[bevy_main]` generates ALL of that from a plain `fn main()` — the `no_mangle` export, the
//! `AndroidApp` handle, and the `bevy::android::ANDROID_APP` static (note: `bevy::android`, not
//! `bevy::winit` — a real, separate module, per the same source). There is no static to set by
//! hand, no `AndroidApp` type to name, and no `android_main` signature to get right, because none
//! of that is this file's job any more. This file's only real job, same as before, is the two env
//! vars (see below) — set inside `main()`, before anything that reads them.
//!
//! `#[bevy_main]` requires the function be named exactly `main` (it asserts this and fails to
//! compile otherwise — see the macro source above), so this can no longer be named anything else
//! or restructured as a library function called from elsewhere.

use benilla_app::BuildId;

/// Subdirectory names inside the app's external files dir. Matched to what a player finds if
/// they browse there with a file manager — kept identical in spirit to the desktop
/// `WOW_DATA`/`benilla-config` names so the two platforms document the same way.
const WOWDATA_DIRNAME: &str = "WOWDATA";
const CONFIG_DIRNAME: &str = "benilla-config";

#[bevy::prelude::bevy_main]
fn main() {
    // Desktop has a real shell environment; Android has none. `std::env::set_var` still works —
    // it's process-local state, not a shell feature — so setting it here, first thing, is
    // sufficient; nothing downstream needs to know it's running on Android instead of reading a
    // real env var a launcher script set.
    //
    // IMPORTANT: this deliberately does NOT use `Android/data/<package>/files/...`
    // (`AndroidApp::external_data_path()`), despite that being the "obvious" no-permission app
    // storage dir and despite an EARLIER version of this file using exactly that. Confirmed by
    // hands-on device testing (not assumed): as of Android 11 (API 30), `Android/data/<pkg>/...`
    // is blocked at the storage layer for every app except the owner — no file manager, including
    // third-party ones, can browse into it, regardless of manifest permissions or a
    // DocumentsProvider. `Android/obb` and `Android/media` are the only two subtrees Android
    // exempted from that lockdown. `Android/media/<package>/...` is used here — still no runtime
    // storage permission required, and (per on-device testing) actually visible and browsable in
    // real file managers, unlike `Android/data`.
    //
    // The path is constructed directly (`/sdcard/Android/media/<package>`) rather than queried
    // via `Context.getExternalMediaDirs()`, because `android-activity` 0.6.1 does not expose that
    // call — only `external_data_path()`/`internal_data_path()`/`obb_path()` are bound (confirmed
    // against android-activity's own docs.rs page). Getting the OS-reported path instead of this
    // constructed one would mean a manual JNI call through `app.vm_as_ptr()` (the same pattern
    // this file's header doc-comment already shows for the Toast example) — flagged as a
    // follow-up, not done here, since it needs a new `jni` crate dependency and cannot be
    // compile-verified without real device/CI access. `Android/media/<package>` (no `/files`
    // suffix — that's an `Android/data`-specific convention, not shared by `Android/media`) is
    // the standard, documented layout on the primary external volume on the overwhelming majority
    // of real devices, same tier of "known-good default, not an OS guarantee" as the previous
    // `Android/data` guess was — the difference is this one is confirmed actually reachable.
    let package = "com.benilla.twow"; // must match benilla-android/Cargo.toml's [package.metadata.android] package id
    let base = std::path::PathBuf::from(format!("/sdcard/Android/media/{package}"));

    let wow_data = base.join(WOWDATA_DIRNAME);
    let benilla_home = base.join(CONFIG_DIRNAME);

    for dir in [&wow_data, &benilla_home] {
        if let Err(e) = std::fs::create_dir_all(dir) {
            // No logger is guaranteed set up this early — eprintln! reaches logcat on Android
            // (stdout/stderr are captured), unlike a bare `log::error!` with no subscriber
            // installed, which would silently do nothing.
            eprintln!("android: could not create {}: {e}", dir.display());
        }
    }

    std::env::set_var("WOW_DATA", &wow_data);
    std::env::set_var("BENILLA_HOME", &benilla_home);

    // `#[bevy_main]` asserts this function is named `main` and wraps it in a plain
    // `fn android_main(...)` with no return value — it does not forward a return type the way
    // desktop's `fn main() -> AppExit` (crates/benilla/src/main.rs) does. `benilla_app::run()`
    // still returns `AppExit`; discard it explicitly (`let _ =`) rather than trying to return it,
    // since there is no real process exit code to report it to on Android and returning it would
    // fail to compile against what the macro generates. If distinguishing exit reasons ever
    // matters on Android (crash vs. clean quit vs. app-switch), that would need to be surfaced
    // through something Android-specific instead — not attempted here.
    let _ = benilla_app::run(BuildId {
        sha: env!("BENILLA_GIT_SHA"),
        short: env!("BENILLA_GIT_SHORT"),
        date: env!("BENILLA_GIT_DATE"),
        profile: env!("BENILLA_PROFILE"),
    });
}

