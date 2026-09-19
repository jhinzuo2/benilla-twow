//! Triggers the Android soft keyboard when a text field opens, and dismisses it when the field
//! closes — issue's touch-enhancement ask ("automatically trigger the native Android keyboard
//! whenever a text input field opens (such as login, password, in-game chat, or the macro box)").
//!
//! Two independent "a text field has focus" signals feed this, because there are two independent
//! text-input systems in this client and neither is aware of the other:
//!
//! - [`UiKeyboardCapture::typing`]: the single flag the whole in-game FrameXML `EditBoxState`
//!   system already drives while a chat box, the macro editor, or any other addon `<EditBox>` has
//!   focus (`ui_script`'s own doc on the field: "there is at most one focused box"). One flag
//!   covers every FrameXML text field the client has — chat and the macro box among them — hooked
//!   here once rather than at each of their call sites.
//! - The login screen's own `LoginForm` (`crate::login`): a *separate*, simpler glue-widget text
//!   system the FrameXML flag above does not reach. Its `Field` has no "nothing focused" state —
//!   only `Account` or `Password` — so on that screen a login field is *always* logically
//!   focused, and "should the keyboard be up" reduces to "is the login screen the active
//!   [`ClientState`]".
//!
//! **Not covered**: `char_create`'s name box is a third, separate glue-widget text field (its own
//! `focused` state, not `LoginForm`'s) and is not wired in here — flagged as a follow-up rather
//! than guessed at, since getting its exact focus field wrong would silently misfire the keyboard
//! on that one screen instead of failing to compile where it would be caught.
//!
//! Off Android this whole plugin is a deliberate no-op (see the two [`keyboard_edge`] bodies
//! below) rather than something every caller gates with its own `#[cfg]` — the same split
//! `textinput::clipboard` already uses for its own platform backends.

use bevy::prelude::*;

use crate::char_select::ClientState;
use crate::ui_script::UiKeyboardCapture;

pub(crate) struct AndroidKeyboardPlugin;

impl Plugin for AndroidKeyboardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WasTyping>()
            .add_systems(Update, drive_soft_keyboard);
    }
}

/// Last frame's answer, so the real JNI call in [`keyboard_edge`] fires only on the transition —
/// not every frame regardless of whether anything changed.
#[derive(Resource, Default)]
struct WasTyping(bool);

fn drive_soft_keyboard(
    capture: Res<UiKeyboardCapture>,
    client_state: Res<State<ClientState>>,
    mut was_typing: ResMut<WasTyping>,
) {
    let typing = capture.typing || *client_state.get() == ClientState::Login;
    if typing == was_typing.0 {
        return;
    }
    was_typing.0 = typing;
    keyboard_edge(typing);
}

/// Show or hide the real IME. `show_implicit`/`hide_implicit_only` both `false`: this is an
/// explicit trigger from a focus change this module tracked itself, not the OS's own
/// implicit-focus heuristic, so neither flag's "only if implicit" carve-out applies.
#[cfg(target_os = "android")]
fn keyboard_edge(show: bool) {
    let Some(app) = bevy::android::ANDROID_APP.get() else {
        // Before `#[bevy_main]` has set the handle (very early startup) — nothing to call yet;
        // `drive_soft_keyboard` runs every frame this plugin is active, so the next transition
        // (or the next frame, if `typing` is already true once the handle lands) retries.
        return;
    };
    if show {
        app.show_soft_input(false);
    } else {
        app.hide_soft_input(false);
    }
}

/// Every other target: the whole point of the split above is that no caller needs its own
/// `#[cfg(target_os = "android")]` — `drive_soft_keyboard` runs everywhere, unconditionally, and
/// simply has nothing to do off Android.
#[cfg(not(target_os = "android"))]
fn keyboard_edge(_show: bool) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn keyboard_app() -> App {
        let mut app = App::new();
        app.init_state::<ClientState>()
            .init_resource::<UiKeyboardCapture>()
            .add_plugins(AndroidKeyboardPlugin);
        app
    }

    /// `ClientState` defaults to `Login` (see its own `#[default]`), so the driver should read
    /// "typing" from the very first frame — before anything ever sets `UiKeyboardCapture::typing`
    /// — since a login field is always logically focused on that screen.
    #[test]
    fn the_login_screen_starts_typing_with_no_frame_focused() {
        let mut app = keyboard_app();
        app.update();
        assert!(app.world().resource::<WasTyping>().0);
    }

    /// Leaving the login screen for character select, with no FrameXML box focused either, drops
    /// back to "not typing" — the keyboard should not stay pinned open once there is nothing left
    /// on screen that reads its input.
    #[test]
    fn leaving_login_with_nothing_else_focused_stops_typing() {
        let mut app = keyboard_app();
        app.update();
        app.world_mut()
            .resource_mut::<NextState<ClientState>>()
            .set(ClientState::CharSelect);
        app.update();
        assert!(!app.world().resource::<WasTyping>().0);
    }

    /// A FrameXML box (chat, the macro editor, …) taking focus on a *non*-login screen is the
    /// other half of the OR — it alone should be enough to raise `typing`.
    #[test]
    fn a_framexml_box_focused_outside_login_still_types() {
        let mut app = keyboard_app();
        app.world_mut()
            .resource_mut::<NextState<ClientState>>()
            .set(ClientState::CharSelect);
        app.update(); // land on CharSelect, not typing yet
        assert!(!app.world().resource::<WasTyping>().0);
        app.world_mut()
            .resource_mut::<UiKeyboardCapture>()
            .typing = true;
        app.update();
        assert!(app.world().resource::<WasTyping>().0);
    }
}
