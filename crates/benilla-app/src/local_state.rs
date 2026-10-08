//! Where benilla keeps local state: one folder, `benilla-config/`, never inside the install, and
//! this module is the only place that computes a path into it; each path fn's doc says what it
//! holds. The 1.12 client is portable the same way, its `WTF/` and `Cache/` in its own folder.
//!
//! Resolution, in order, the same shape as [`benilla_formats::wow_data`]:
//! 1. `$BENILLA_HOME`.
//! 2. `<project folder>/benilla-config/`, dev builds only ([`benilla_formats::project_folder`], the
//!    launcher's): a shipped binary must not carry the build machine's source tree.
//! 3. `<exe dir>/benilla-config/`.
//!
//! With `$WOW_CAPTURE` set every path resolves to `None`: a capture neither reads nor writes
//! player state.

use std::io::Write;
use std::path::{Path, PathBuf};

/// The folder's name: not `benilla`, which beside the binary is the executable itself, and not
/// `WTF`, which a real install already uses.
const STATE_DIR: &str = "benilla-config";

/// The state folder, or `None` when persistence is off (a capture run, or no executable path).
/// It may not exist yet: [`write_atomic`] creates it, and a reader treats a missing file as
/// defaults.
pub(crate) fn home() -> Option<PathBuf> {
    if std::env::var_os("WOW_CAPTURE").is_some() {
        return None; // hermetic: captures neither read nor write player state
    }
    if let Some(over) = std::env::var_os("BENILLA_HOME") {
        return Some(PathBuf::from(over));
    }
    if cfg!(test) {
        return None; // a unit test reaches no real state folder unless it pins `$BENILLA_HOME`
    }
    // Windows: `Documents\benilla-twow\benilla-config`, unconditionally — dev and player builds
    // alike, ahead of steps 3/4 below rather than added after them. Both of those steps name a
    // path known only to the machine that COMPILED the binary — step 3 bakes in
    // `CARGO_MANIFEST_DIR`, step 4 resolves next to whatever `current_exe()` reports — and on
    // every other platform that is exactly the intended behaviour (§3/§4's own doc comments).
    // Windows breaks that assumption in a way the other platforms don't: a `dev`-feature binary
    // built on a GitHub Actions Windows runner and handed to a player carries that runner's own
    // checkout path baked in at compile time (`D:\a\<repo>\<repo>\benilla-config`) — a directory
    // that exists on no machine but the one that built it, and Windows will happily create it in
    // place given a `D:` drive of some description exists. This is not a hardening step in the
    // 0954/1175 sense (that decision record's reasoning against a hidden platform config dir
    // still holds, and Documents is user-visible, so it is not in tension with it) — it exists
    // because this bug class doesn't reproduce anywhere the CI/local checkout path happens to
    // match the running machine's, which on macOS/Linux dev boxes it usually does, and on a
    // Windows player's machine it never can. Not gated behind `#[cfg(feature = "dev")]`: a
    // `ship`-profile (`--no-default-features`) Windows binary has the same bug one step later,
    // landing beside whatever folder the player happened to unzip it into rather than a build
    // path — less alarming, but just as much "wherever it got compiled/unpacked" as the dev case,
    // and just as much not what a player expects. `windows_documents_dir` returning `None`
    // (SHGetKnownFolderPath failing, or a caller on a matching test path) falls through to
    // steps 3/4 as before rather than losing persistence outright.
    #[cfg(windows)]
    if let Some(docs) = windows_documents_dir() {
        return Some(docs.join("benilla-twow").join(STATE_DIR));
    }
    resident_home()
}

/// Steps 2 and 3 of [`home`]: the folder the build itself lives in.
fn resident_home() -> Option<PathBuf> {
    // 2 · the project folder, dev builds only.
    if let Some(root) = dev_project_root() {
        return Some(root.join(STATE_DIR));
    }
    // 3 · beside the binary; a dev build never reaches here.
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
        .map(|dir| dir.join(STATE_DIR))
}


/// The current user's Documents folder via the shell API Microsoft documents for exactly this
/// (`FOLDERID_Documents`), not `%USERPROFILE%\Documents` string-glued by hand: a redirected or
/// OneDrive-relocated Documents folder — both common, neither the player's fault — lives somewhere
/// else entirely, and `SHGetKnownFolderPath` is the one call that already knows where. `None` on
/// any failure (the call erroring, or the returned pointer being null); callers fall back to the
/// existing resolution rather than treating that as fatal.
#[cfg(windows)]
fn windows_documents_dir() -> Option<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::S_OK;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_Documents, SHGetKnownFolderPath};

    unsafe {
        let mut raw: *mut u16 = std::ptr::null_mut();
        // dwFlags = 0 (KF_FLAG_DEFAULT: no special handling — the ordinary, possibly-redirected
        // path); hToken = null (the calling process's own user, no impersonation) — HANDLE is
        // `*mut c_void` in windows-sys 0.59, not an integer, hence `null_mut()` rather than `0`.
        let hr = SHGetKnownFolderPath(&FOLDERID_Documents, 0, std::ptr::null_mut(), &mut raw);
        if hr != S_OK || raw.is_null() {
            return None;
        }
        let len = {
            let mut n = 0isize;
            while *raw.offset(n) != 0 {
                n += 1;
            }
            n as usize
        };
        let wide = std::slice::from_raw_parts(raw, len);
        let path = PathBuf::from(OsString::from_wide(wide));
        CoTaskMemFree(raw as *const core::ffi::c_void);
        Some(path)
    }
}

/// The project folder a dev build keeps its state in ([`benilla_formats::project_folder`]), through
/// [`shared_root`].
fn dev_project_root() -> Option<PathBuf> {
    Some(shared_root(benilla_formats::project_folder()?))
}

/// `here`, or for a linked worktree its primary checkout, whichever worktree built the binary, so
/// every worktree shares one settings folder. Anything unexpected keeps `here`.
fn shared_root(here: &Path) -> PathBuf {
    let dot_git = here.join(".git");
    // A linked worktree: `.git` is a file pointing at `<primary>/.git/worktrees/<name>`.
    if dot_git.is_file() {
        if let Some(gitdir) = std::fs::read_to_string(&dot_git).ok().and_then(|s| {
            s.trim()
                .strip_prefix("gitdir:")
                .map(|p| p.trim().to_owned())
        }) {
            // …/.git/worktrees/<name> → …/.git → the primary checkout.
            if let Some(primary) = Path::new(&gitdir)
                .ancestors()
                .nth(2)
                .and_then(|d| d.parent())
            {
                if primary.is_dir() {
                    return primary.to_path_buf();
                }
            }
        }
    }
    here.to_path_buf()
}

/// `benilla-config/config.toml`: the CVar overrides, the `Config.wtf` analog.
pub(crate) fn config_path() -> Option<PathBuf> {
    home().map(|h| h.join("config.toml"))
}

/// `benilla-config/macros/account.txt`: the account-wide macro tab (indices 1..=18).
pub(crate) fn macros_account_path() -> Option<PathBuf> {
    home().map(|h| h.join("macros/account.txt"))
}

/// `benilla-config/macros/<realm>-<character>.txt`: the per-character macro tab (19..=36), the
/// reference's `WTF/Account/<ACC>/<REALM>/<CHAR>/macros-cache.txt` flattened into one folder.
pub(crate) fn macros_character_path(realm: &str, character: &str) -> Option<PathBuf> {
    let key = format!("{}-{}", file_token(realm), file_token(character));
    home().map(|h| h.join("macros").join(format!("{key}.txt")))
}

/// `benilla-config/bindings/account.txt`: the account-wide key bindings (the `bindings-cache.wtf`
/// analog), a diff against the defaults.
pub(crate) fn bindings_account_path() -> Option<PathBuf> {
    home().map(|h| h.join("bindings/account.txt"))
}

/// `benilla-config/bindings/<realm>-<character>.txt`: the character-specific binding set. Its
/// existence is the "character specific key bindings" state; going back to general deletes it.
pub(crate) fn bindings_character_path(realm: &str, character: &str) -> Option<PathBuf> {
    let key = format!("{}-{}", file_token(realm), file_token(character));
    home().map(|h| h.join("bindings").join(format!("{key}.txt")))
}

/// `benilla-config/saved-variables.lua`: the flat channel our FrameXML saves through
/// `RegisterForSave` (the `SavedVariables.lua` analog), written whole at logout, run at UI load.
pub(crate) fn saved_variables_path() -> Option<PathBuf> {
    home().map(|h| h.join("saved-variables.lua"))
}

/// `benilla-config/addons/<Realm>-<Character>.txt`: the AddOn enable state, per character in the
/// reference's `AddOns.txt` format (`<AddOnName>: enabled|disabled`), which it writes at the tail
/// of its UI shutdown (`0x490bd0`). An addon absent from the file is enabled.
pub(crate) fn addons_state_path(realm: &str, character: &str) -> Option<PathBuf> {
    let key = format!("{}-{}", file_token(realm), file_token(character));
    home().map(|h| h.join("addons").join(format!("{key}.txt")))
}

/// `benilla-config/saved/`: per-addon saved variables, account scope, one `<Addon>.lua` per addon
/// declaring `## SavedVariables` (the reference's `WTF/Account/<ACC>/SavedVariables/<Addon>.lua`).
pub(crate) fn addon_saved_account_dir() -> Option<PathBuf> {
    home().map(|h| h.join("saved"))
}

/// `benilla-config/saved/<Realm>-<Character>/`: per-addon saved variables, character scope
/// (`## SavedVariablesPerCharacter`), loaded after the account file so it wins.
pub(crate) fn addon_saved_character_dir(realm: &str, character: &str) -> Option<PathBuf> {
    let key = format!("{}-{}", file_token(realm), file_token(character));
    home().map(|h| h.join("saved").join(key))
}

/// `benilla-config/camera/<realm>-<character>.txt`: the third-person camera pose (the
/// `<Char>/camera-settings.txt` analog), two lines in the reference's keys and order.
pub(crate) fn camera_character_path(realm: &str, character: &str) -> Option<PathBuf> {
    let key = format!("{}-{}", file_token(realm), file_token(character));
    home().map(|h| h.join("camera").join(format!("{key}.txt")))
}

/// `benilla-config/account`: the account name the login screen remembers
/// (`GetSavedAccountName`/`SetSavedAccountName`; the reference's is `Config.wtf`'s `accountName`).
pub(crate) fn saved_account_path() -> Option<PathBuf> {
    home().map(|h| h.join("account"))
}

/// `benilla-config/chat/<realm>-<character>.txt`: the chat windows' tint, alpha, font size and
/// lock, the four the reference keeps in its per-character `chat-cache.txt`.
/// `benilla-config/password` — the password the login screen's Remember Password box keeps, beside
/// [`saved_account_path`]'s name and read back only together with it. Ours: the reference has no
/// such file (its own client remembers a password through the saved-account list, which is not
/// built here).
///
/// **Stored in clear** — see `login::save_password_to` for the trade and the owner-only mode it is
/// written with. Through [`home`], so it inherits the same hermetic guard as the account name: a
/// capture reads no saved password, and cannot photograph one into the frame.
pub(crate) fn saved_password_path() -> Option<PathBuf> {
    home().map(|h| h.join("password"))
}

/// `benilla-config/chat/<realm>-<character>.txt`: the chat windows' tint, alpha, font size and
/// lock, the four the reference keeps in its per-character `chat-cache.txt`.
pub(crate) fn chat_character_path(realm: &str, character: &str) -> Option<PathBuf> {
    Some(home()?.join("chat").join(format!(
        "{}-{}.txt",
        file_token(realm),
        file_token(character)
    )))
}

/// `benilla-config/layout/<realm>-<character>.txt`: the layout cache, the geometry of every
/// user-placed frame, as the reference's per-character `layout-cache.txt`; [`crate::ui_layout`]
/// owns the shape.
pub(crate) fn layout_character_path(realm: &str, character: &str) -> Option<PathBuf> {
    Some(home()?.join("layout").join(format!(
        "{}-{}.txt",
        file_token(realm),
        file_token(character)
    )))
}

/// `benilla-config/cache/<realm>.tsv`: the player, creature and pet names the server has answered,
/// one file for the reference's `WDB/namecache.wdb`, `creaturecache.wdb` and `petnamecache.wdb`.
///
/// Realm-scoped, where the reference's is not: guids, creature entries and pet numbers are
/// realm-local, so one shared file would serve a realm another realm's names.
pub(crate) fn name_cache_path(realm: &str) -> Option<PathBuf> {
    Some(
        home()?
            .join("cache")
            .join(format!("{}.tsv", file_token(realm))),
    )
}

/// `benilla-config/Logs/`: `WoWChatLog.txt` and `WoWCombatLog.txt`, the reference's names.
pub(crate) fn logs_dir() -> Option<PathBuf> {
    home().map(|h| h.join("Logs"))
}

/// `benilla-config/shots.txt`: the framing instrument's camera poses (`/shot`, dev builds).
pub(crate) fn shots_path() -> Option<PathBuf> {
    home().map(|h| h.join("shots.txt"))
}

/// `benilla-config/Screenshots/`: where the print-screen key writes, each image once, not through
/// [`write_atomic`].
///
/// Deviation: the reference writes `Screenshots\\` inside the install, which benilla never writes
/// to; the folder keeps the reference's name.
pub(crate) fn screenshots_dir() -> Option<PathBuf> {
    home().map(|h| h.join("Screenshots"))
}

/// `benilla-config/Diagnostics/`: the stuck-thread sampler's profiles ([`crate::perf::stall`]) and
/// the FPS journal; `None` on a capture run like every path here, so a capture has no sampler.
pub(crate) fn diagnostics_dir() -> Option<PathBuf> {
    home().map(|h| h.join("Diagnostics"))
}

/// `benilla-config/Diagnostics/fps-journal.csv`: the FPS journal's rows while the `fpsJournal`
/// CVar is on; a capture names its own path through `WOW_FPS_JOURNAL`.
pub(crate) fn fps_journal_path() -> Option<PathBuf> {
    diagnostics_dir().map(|d| d.join("fps-journal.csv"))
}

/// A realm or character name as one path component: anything not a letter or digit becomes `_`.
/// Letters of any script stay, as in the reference's raw-name folders: vmangos accepts non-Latin
/// names (`StrictPlayerNames = 0`) and a character name is letters only, so no two share a file.
fn file_token(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    if t.is_empty() {
        "unknown".into()
    } else {
        t
    }
}

/// Write a state file atomically, through `<path>.tmp` and a rename, so a crash leaves the old
/// file intact.
pub(crate) fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    write_atomic_bytes(path, contents.as_bytes())
}

/// [`write_atomic`] for bytes: the saved-variables files carry Lua byte strings as they are.
pub(crate) fn write_atomic_bytes(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(contents)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

/// Test plumbing for the persistence env vars: `set_var` is process-global, so every such test
/// takes [`ENV_LOCK`] and scopes its overrides in [`EnvGuard`]s.
#[cfg(test)]
pub(crate) mod test_env {
    pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    pub(crate) struct EnvGuard(&'static str, Option<std::ffi::OsString>);
    impl EnvGuard {
        pub(crate) fn set(key: &'static str, value: &str) -> Self {
            let old = std::env::var_os(key);
            std::env::set_var(key, value);
            Self(key, old)
        }
        pub(crate) fn unset(key: &'static str) -> Self {
            let old = std::env::var_os(key);
            std::env::remove_var(key);
            Self(key, old)
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.1 {
                Some(v) => std::env::set_var(self.0, v),
                None => std::env::remove_var(self.0),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_env::{EnvGuard, ENV_LOCK};
    use super::*;

    /// Every per-character file keys on this token; ASCII names map unchanged.
    #[test]
    fn distinct_names_never_share_a_token() {
        assert_ne!(file_token("Вася"), file_token("Петя"));
        assert_ne!(file_token("Zoë"), file_token("Zoé"));
        assert_eq!(file_token("Zoë"), "Zoë");
        assert_eq!(file_token("Onehunter"), "Onehunter");
        assert_eq!(file_token("Hydraxian Waterlords"), "Hydraxian_Waterlords");
        assert_eq!(
            file_token("a/b\\c:d"),
            "a_b_c_d",
            "separators never survive"
        );
        assert_eq!(
            file_token("realm-name"),
            "realm_name",
            "the key's own `-` stays unforgeable"
        );
    }

    /// The layout under the override, the one step of [`home`] whose answer a test can state.
    #[test]
    fn the_home_law_override_then_the_residents_and_hermetic_captures() {
        let _l = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let tmp = std::env::temp_dir().join(format!("benilla-ls-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();

        // 1 · the explicit override.
        let _c = EnvGuard::unset("WOW_CAPTURE");
        let _h = EnvGuard::set("BENILLA_HOME", tmp.join(STATE_DIR).to_str().unwrap());
        assert_eq!(home(), Some(tmp.join(STATE_DIR)));
        assert_eq!(config_path(), Some(tmp.join("benilla-config/config.toml")));
        assert_eq!(
            macros_account_path(),
            Some(tmp.join("benilla-config/macros/account.txt"))
        );
        assert_eq!(
            saved_variables_path(),
            Some(tmp.join("benilla-config/saved-variables.lua"))
        );
        assert_eq!(
            macros_character_path("Hydraxian Waterlords", "Probeone"),
            Some(tmp.join("benilla-config/macros/Hydraxian_Waterlords-Probeone.txt"))
        );
        assert_eq!(
            macros_character_path("../evil", "a/b"),
            Some(tmp.join("benilla-config/macros/___evil-a_b.txt")),
            "no name can escape the folder"
        );
        assert_eq!(
            camera_character_path("Hydraxian Waterlords", "Probeone"),
            Some(tmp.join("benilla-config/camera/Hydraxian_Waterlords-Probeone.txt"))
        );
        assert_eq!(
            chat_character_path("Hydraxian Waterlords", "Probeone"),
            Some(tmp.join("benilla-config/chat/Hydraxian_Waterlords-Probeone.txt"))
        );
        assert_eq!(
            layout_character_path("Hydraxian Waterlords", "Probeone"),
            Some(tmp.join("benilla-config/layout/Hydraxian_Waterlords-Probeone.txt"))
        );

        assert_eq!(
            saved_account_path(),
            Some(tmp.join("benilla-config/account"))
        );
        // ...and the password the Remember Password box keeps beside it.
        assert_eq!(
            saved_password_path(),
            Some(tmp.join("benilla-config/password"))
        );
        assert_eq!(shots_path(), Some(tmp.join("benilla-config/shots.txt")));
        assert_eq!(
            screenshots_dir(),
            Some(tmp.join("benilla-config/Screenshots"))
        );

        // 0 · a capture run resolves nothing, even with an override set.
        let _c2 = EnvGuard::set("WOW_CAPTURE", "ui-options");
        assert_eq!(home(), None);
        assert_eq!(
            saved_account_path(),
            None,
            "a capture reads no saved account"
        );
        assert_eq!(
            saved_password_path(),
            None,
            "a capture reads no saved password"
        );
        assert_eq!(shots_path(), None);
        assert_eq!(
            screenshots_dir(),
            None,
            "a capture writes no player screenshots"
        );
        assert_eq!(
            chat_character_path("Hydraxian Waterlords", "Probeone"),
            None,
            "a capture reads no player's chat look"
        );
        assert_eq!(
            layout_character_path("Hydraxian Waterlords", "Probeone"),
            None,
            "a capture reads no player's window layout"
        );
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// A unit test that does not pin `$BENILLA_HOME` resolves no state folder, so no test can write
    /// the real one.
    #[test]
    fn a_test_without_an_override_has_no_state_folder() {
        let _l = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _c = EnvGuard::unset("WOW_CAPTURE");
        let _h = EnvGuard::unset("BENILLA_HOME");
        assert_eq!(home(), None);
        assert_eq!(config_path(), None);
    }

    /// A missing or wrong `$WOW_DATA` does not move or lose the state folder.
    #[test]
    fn the_state_folder_no_longer_depends_on_finding_the_install() {
        let _l = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _c = EnvGuard::unset("WOW_CAPTURE");
        let _h = EnvGuard::unset("BENILLA_HOME");
        let _d = EnvGuard::set("WOW_DATA", "/nonexistent/benilla-test/Data");
        let h =
            resident_home().expect("a broken install path must not cost the player their config");
        assert!(
            !h.starts_with("/nonexistent"),
            "home() still reads $WOW_DATA: {}",
            h.display()
        );
        assert!(h.ends_with(STATE_DIR), "{}", h.display());
    }

    /// A player build resolves beside the binary; a dev build to the primary checkout, even from a
    /// linked worktree (whose `.git` is a file).
    #[test]
    fn the_state_folder_lands_where_the_build_says() {
        let _l = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _c = EnvGuard::unset("WOW_CAPTURE");
        let _h = EnvGuard::unset("BENILLA_HOME");
        let h = resident_home().expect("the resident folder always resolves");
        assert!(h.ends_with(STATE_DIR), "{}", h.display());

        #[cfg(windows)]
        {
            // `home()` tries this before either of the other two steps, so as long as the shell
            // call succeeds (true on every CI/player Windows box — it fails only in exotic
            // sandboxes with no Documents folder at all) this is the whole answer, and the
            // project-folder/exe-dir cases below are unreachable on this platform.
            if let Some(docs) = windows_documents_dir() {
                assert_eq!(h, docs.join("benilla-twow").join(STATE_DIR));
                return;
            }
        }

        let Some(here) = benilla_formats::project_folder() else {
            // Player build: beside the binary.
            let exe_dir = std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf();
            assert_eq!(h, exe_dir.join(STATE_DIR));
            return;
        };

        if here.join(".git").is_file() {
            assert_ne!(
                h,
                here.join(STATE_DIR),
                "a linked worktree must not get its own settings folder — the worktrees share one"
            );
            assert!(
                h.parent().unwrap().join(".git").is_dir(),
                "resolved to {}, which is not a primary checkout",
                h.display()
            );
        } else {
            assert_eq!(h, here.join(STATE_DIR));
        }
    }

    /// A project folder that is a linked worktree keeps its state in the primary checkout, so every
    /// worktree shares one; any other folder, a crate on top of benilla's included, keeps its own.
    #[test]
    fn the_state_root_is_the_folder_or_its_primary_checkout() {
        let tmp = std::env::temp_dir().join(format!("benilla-root-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        let primary = tmp.join("benilla");
        std::fs::create_dir_all(primary.join(".git/worktrees/pool-3")).unwrap();
        let slot = tmp.join("pool-3");
        std::fs::create_dir_all(&slot).unwrap();
        let gitdir = primary.join(".git/worktrees/pool-3");
        std::fs::write(slot.join(".git"), format!("gitdir: {}\n", gitdir.display())).unwrap();
        assert_eq!(shared_root(&slot), primary);
        assert_eq!(shared_root(&primary), primary);
        let hello_mod = tmp.join("hello-mod");
        std::fs::create_dir_all(hello_mod.join(".git")).unwrap();
        assert_eq!(shared_root(&hello_mod), hello_mod);
        let plain = tmp.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        assert_eq!(shared_root(&plain), plain);
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// A launcher's recorded folder is where a dev build keeps its state and reads its probe
    /// identity; a player build resolves neither from it. Recording is process-wide, so the parent
    /// runs this same test in a child process of the test binary, which records and resolves.
    #[test]
    fn a_recorded_launcher_folder_holds_the_state_and_the_identity() {
        const CHILD: &str = "BENILLA_TEST_LAUNCHER_FOLDER";
        if let Some(dir) = std::env::var_os(CHILD) {
            let dir = PathBuf::from(dir);
            benilla_formats::set_project_folder(dir.to_str().unwrap());
            if crate::run_mode::dev_affordances() {
                assert_eq!(resident_home(), Some(dir.join(STATE_DIR)));
                assert_eq!(
                    crate::run_mode::declared_identity(),
                    Some(crate::run_mode::DeclaredIdentity {
                        user: "probe9".into(),
                        character: "Modchar".into()
                    })
                );
            } else {
                let exe_dir = std::env::current_exe().unwrap();
                assert_eq!(
                    resident_home(),
                    Some(exe_dir.parent().unwrap().join(STATE_DIR))
                );
                assert_eq!(crate::run_mode::declared_identity(), None);
            }
            return;
        }
        let tmp = std::env::temp_dir().join(format!("benilla-launcher-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(
            tmp.join(".probe-identity"),
            "WOW_USER=probe9\nWOW_PASS=unused\nWOW_CHAR=Modchar\n",
        )
        .unwrap();
        let name =
            "local_state::tests::a_recorded_launcher_folder_holds_the_state_and_the_identity";
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture"])
            .env(CHILD, &tmp)
            .env_remove("BENILLA_HOME")
            .env_remove("WOW_CAPTURE")
            .output()
            .unwrap();
        let report = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            out.status.success() && report.contains("1 passed"),
            "the child run:\n{report}"
        );
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn write_atomic_creates_dirs_and_replaces_whole_files() {
        let _l = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let tmp = std::env::temp_dir().join(format!("benilla-wa-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        let path = tmp.join("nested/config.toml");
        write_atomic(&path, "a = \"1\"\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a = \"1\"\n");
        write_atomic(&path, "a = \"2\"\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a = \"2\"\n");
        assert!(!path.with_extension("tmp").exists(), "tmp renamed away");
        std::fs::remove_dir_all(&tmp).ok();
    }
}
