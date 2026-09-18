//! The on-screen stick.
//!
//! Drawn with `bevy_ui` and **not** through the FrameXML/Lua engine, deliberately. The stick is
//! part of the device's input hardware, not part of the game's interface: it must survive
//! TOGGLEUI, it must not be reachable (or breakable) by an addon, it must not consume a click
//! through the UI hit-test — if it did, [`super::classify_touches`] would classify the thumb as
//! [`super::Role::Ui`] and the stick could never claim its own finger — and it must draw before
//! the Lua VM is even alive, since you can steer on the character screen. Every one of those is a
//! reason not to be a Frame.
//!
//! It is a **floating** stick: the base appears centred wherever the thumb lands inside the zone
//! and fades out when it lifts, so there is no fixed target to hunt for.

use bevy::prelude::*;

use super::{JoystickAnchor, JoystickCfg, TouchMove};

/// Marker for the outer ring (the base).
#[derive(Component)]
struct StickBase;

/// Marker for the inner knob.
#[derive(Component)]
struct StickKnob;

/// Above the FrameXML layer, below the boot diagnostic overlay.
const STICK_Z: i32 = 1500;

/// Base diameter as a multiple of the deflection radius. The ring is drawn a little larger than
/// the radius at which the stick reads full travel, so "pushed all the way" happens *inside* the
/// ring rather than at its rim — hitting full deflection with thumb to spare feels correct;
/// hitting it exactly at the edge feels like the stick ran out.
const BASE_SCALE: f32 = 2.4;
const KNOB_SCALE: f32 = 0.9;

pub(super) struct JoystickVisualsPlugin;

impl Plugin for JoystickVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<JoystickVisuals>()
            .add_systems(Startup, spawn_stick)
            .add_systems(Update, drive_stick.after(super::TouchInputSet));
    }
}

/// Marker resource so other modules can ask whether the stick is drawn without reaching for the
/// entities.
#[derive(Resource, Default)]
pub(crate) struct JoystickVisuals {
    pub visible: bool,
}

fn spawn_stick(mut commands: Commands, cfg: Res<JoystickCfg>) {
    if !cfg.enabled {
        return;
    }
    let base = cfg.radius * BASE_SCALE;
    let knob = cfg.radius * KNOB_SCALE;

    commands.spawn((
        StickBase,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(base),
            height: Val::Px(base),
            // Parked off-screen until the first touchdown places it. Spawning it hidden rather
            // than spawning on demand keeps the first engage free of a frame of layout pop.
            left: Val::Px(-9999.0),
            top: Val::Px(-9999.0),
            border: UiRect::all(Val::Px(2.0)),
            ..default()
        },
        BorderRadius::MAX,
        BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.25)),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.18)),
        GlobalZIndex(STICK_Z),
        // No `Button`, no `Interaction`: this must never take a pointer. See the module doc.
        Pickable::IGNORE,
    ));

    commands.spawn((
        StickKnob,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(knob),
            height: Val::Px(knob),
            left: Val::Px(-9999.0),
            top: Val::Px(-9999.0),
            ..default()
        },
        BorderRadius::MAX,
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.35)),
        GlobalZIndex(STICK_Z + 1),
        Pickable::IGNORE,
    ));
}

#[expect(
    clippy::type_complexity,
    reason = "two disjoint queries over the same component set; the filter pair is the whole point"
)]
fn drive_stick(
    cfg: Res<JoystickCfg>,
    anchor: Res<JoystickAnchor>,
    mv: Res<TouchMove>,
    mut visuals: ResMut<JoystickVisuals>,
    mut q: ParamSet<(
        Query<(&mut Node, &mut BackgroundColor), With<StickBase>>,
        Query<(&mut Node, &mut BackgroundColor), With<StickKnob>>,
    )>,
) {
    if !cfg.enabled {
        return;
    }
    let base = cfg.radius * BASE_SCALE;
    let knob = cfg.radius * KNOB_SCALE;

    let Some(centre) = anchor.centre else {
        visuals.visible = false;
        // Park both off-screen. Setting `Display::None` instead would be cheaper, but it also
        // drops them out of layout entirely, and re-entering layout on the next touchdown is the
        // one frame of pop this is avoiding.
        if let Ok((mut n, mut bg)) = q.p0().single_mut() {
            n.left = Val::Px(-9999.0);
            n.top = Val::Px(-9999.0);
            bg.0.set_alpha(0.0);
        }
        if let Ok((mut n, mut bg)) = q.p1().single_mut() {
            n.left = Val::Px(-9999.0);
            n.top = Val::Px(-9999.0);
            bg.0.set_alpha(0.0);
        }
        return;
    };

    visuals.visible = true;

    // The base sits centred on the anchor.
    if let Ok((mut n, mut bg)) = q.p0().single_mut() {
        n.left = Val::Px(centre.x - base * 0.5);
        n.top = Val::Px(centre.y - base * 0.5);
        bg.0.set_alpha(0.18);
    }

    // The knob rides the analog vector, clamped to the ring. `analog.y` is y-up (forward is
    // positive) and the UI is y-down, so it is negated on the way out.
    let offset = Vec2::new(mv.analog.x, -mv.analog.y) * cfg.radius;
    if let Ok((mut n, mut bg)) = q.p1().single_mut() {
        n.left = Val::Px(centre.x + offset.x - knob * 0.5);
        n.top = Val::Px(centre.y + offset.y - knob * 0.5);
        // A touch brighter once it is actually deflected, so the stick reads as "engaged" at a
        // glance without needing to see the knob's exact position.
        bg.0
            .set_alpha(if mv.analog.length() > 0.01 { 0.55 } else { 0.35 });
    }
}
