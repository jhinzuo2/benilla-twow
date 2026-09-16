//! The Android launcher shim — the `android_main` twin of `crates/benilla/src/main.rs`.
//!
//! Desktop has a real `fn main()` and a real shell environment. Android has neither: the JVM
//! loads this as a `cdylib`, the `android-activity` glue bevy itself depends on drives lifecycle
//! callbacks, and `#[no_mangle] android_main(AndroidApp)` is the closest thing to an entry point.
//!
//! Two things have to happen here before `benilla_app::run()` is called, since `run()` itself is
//! untouched and knows nothing about Android:
//!
//! 1. **`WOW_DATA` / `BENILLA_HOME`** — both are ordinarily read from a real shell environment
//!    (`local_state::home()` step 1, `benilla_formats::install` step 1) that doesn't exist here.
//!    `std::env::set_var` still works on Android — it's process-local state, not a shell feature
//!    — so setting it early is sufficient; nothing downstream needs to know it's running on
//!    Android instead of reading a real env var a launcher script set. Both point at this app's
//!    own external files dir — `Android/data/<package>/files/...` — which needs no runtime
//!    storage permission on modern Android and is reachable from any file manager, so the player
//!    drops their 1.12.1 `Data/` folder in `.../files/WOWDATA/` the same way they'd point
//!    `WOW_DATA` at it on desktop.
//!
//! 2. **`bevy_winit::ANDROID_APP`** — the `AndroidApp` handle this function receives has to reach
//!    `WinitPlugin` before it builds its `EventLoop`, which happens somewhere inside
//!    `benilla_app::run()`. bevy_winit does this through a `pub static OnceLock<AndroidApp>` it
//!    reads internally rather than a constructor argument, so the whole job on this end is one
//!    `.set()` call before `run()` — see the call site below for the sourcing on this, since it's
//!    the one piece of this file that isn't just "the desktop shim plus two env vars".

use benilla_app::BuildId;

// Re-exported by bevy_winit, not pulled in directly by this crate — see the Cargo.toml comment
// on why a second `android-activity` dependency here would be wrong. Confirmed against
// bevy_winit's own source (crates/bevy_winit/src/lib.rs): `AndroidApp` is `winit`'s type,
// forwarded through bevy so callers never need their own copy of the glue crate.
use bevy::winit::AndroidApp;

/// Subdirectory names inside the app's external files dir. Matched to what a player finds if
/// they browse there with a file manager — kept identical in spirit to the desktop
/// `WOW_DATA`/`benilla-config` names so the two platforms document the same way.
const WOWDATA_DIRNAME: &str = "WOWDATA";
const CONFIG_DIRNAME: &str = "benilla-config";

#[no_mangle]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );

    // `android-activity` exposes the external files dir path itself (it's the same one
    // `Context.getExternalFilesDir(null)` returns on the Java side) — no JNI needed for this
    // part. If this ever comes back `None` on a real device (some OEM skins have done stranger
    // things), the fallback is an internal-storage path, which still works for read/write but
    // isn't reachable by a file manager without adb, so surface that loudly rather than silently
    // writing somewhere the player can't find.
    let base = app
        .external_data_path()
        .unwrap_or_else(|| {
            log::warn!(
                "android: no external_data_path — falling back to internal storage; \
                 the player will not be able to drop files in via a file manager without adb"
            );
            app.internal_data_path().expect(
                "android: neither external nor internal data path available — nowhere to run from",
            )
        });

    let wow_data = base.join(WOWDATA_DIRNAME);
    let benilla_home = base.join(CONFIG_DIRNAME);

    // Create both up front so a fresh install has somewhere for the player to see and drop files
    // into immediately, rather than only appearing after benilla's own lazy-create-on-first-write
    // (see local_state.rs's `home()` doc: "Existence is NOT guaranteed"). A directory a file
    // manager can already see beats one that only appears after the app has run once and failed
    // to find data in it.
    for dir in [&wow_data, &benilla_home] {
        if let Err(e) = std::fs::create_dir_all(dir) {
            log::error!("android: could not create {}: {e}", dir.display());
        }
    }

    // SAFETY / correctness note: this must run before `benilla_app::run` touches either
    // `local_state::home()` or `benilla_formats::install`'s resolver — both read these vars on
    // first call and (per local_state.rs's own doc comment) some paths are cached rather than
    // re-resolved every call. Setting them here, first thing in android_main, is early enough;
    // do not move this after any benilla_app:: call.
    std::env::set_var("WOW_DATA", &wow_data);
    std::env::set_var("BENILLA_HOME", &benilla_home);

    log::info!("android: WOW_DATA={}", wow_data.display());
    log::info!("android: BENILLA_HOME={}", benilla_home.display());

    // The real hand-off, confirmed against bevy_winit's own source: `bevy_winit::ANDROID_APP` is
    // a `pub static OnceLock<AndroidApp>` that `WinitPlugin` reads internally when it builds its
    // `EventLoop` — there is no constructor argument or resource to insert on the `App` side, and
    // no need to call winit's own `EventLoopBuilderExtAndroid::with_android_app` ourselves (that
    // path is for apps that build their own `EventLoop` directly; bevy's runner does it for us).
    // This MUST be set before `benilla_app::run()` below, since that's what builds and runs the
    // `App` — `WinitPlugin` reads the cell once, the first time it constructs the event loop, and
    // a `OnceLock` cannot be reset if we're late.
    bevy::winit::ANDROID_APP
        .set(app)
        .expect("android_main called twice — ANDROID_APP can only be set once");

    benilla_app::run(BuildId {
        sha: env!("BENILLA_GIT_SHA"),
        short: env!("BENILLA_GIT_SHORT"),
        date: env!("BENILLA_GIT_DATE"),
        profile: env!("BENILLA_PROFILE"),
    });
}
