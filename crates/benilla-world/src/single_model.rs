//! One rigged model instance, spawned on its own — eager rig, caller-driven player.
//!
//! Four game lanes put a single model on screen (the character booths, quest markers, UI
//! models, the dressing room) and each assembles it by hand from the engine's pieces; a program
//! with no game — an editor, a viewer — has no lane at all. This is that assembly, once, on the
//! engine side: forms → materials → parts → palette rig → pose buffer → player → emitters and
//! ribbons riding the pose's anchors.
//!
//! What it deliberately is **not**: a placed doodad ([`crate::doodad_anim::spawn_anim_host`]
//! re-rolls the idle variation every play-window and rigs lazily at first draw) or a streamed
//! unit (the game's `creature_anim` decides which clip plays when). Here the **caller** chooses
//! the clip, and the rig is allocated eagerly — a viewer's one model is never a palette-pressure
//! case.
//!
//! **Character models** are supported as far as a *shipped* appearance goes: a caller that knows
//! the `CreatureDisplayInfoExtra` row passes the pre-baked body atlas, the geoset selection and
//! the hair sheet ([`SingleModelSpec::body_atlas`] / [`SingleModelSpec::visible_geosets`] /
//! [`SingleModelSpec::hair_texture`]), and the body renders dressed. What stays out of scope is
//! the **live composite**: a player's (or a bake-less NPC row's) atlas is layered from
//! CharSections + worn region textures by the game's `char_skin`, and nothing here re-implements
//! that — such a subject still draws with an untextured body.

use bevy::camera::visibility::{NoAutoAabb, NoFrustumCulling};
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::mesh::MeshTag;
use bevy::prelude::*;

use benilla_assets::{skin_url, M2Model, ModelAnimations, ModelSubmesh};
use benilla_formats::CharSkinSlot;

use crate::interact::WorldObject;
use crate::mesh_tag::spawn_tag;
use crate::model_forms::FormSlices;
use crate::model_render::{M2BatchMaterials, ModelKind, ModelPart};
use crate::particles::{spawn_emitter, EmitClock, EmitterFade, EmitterFrames, OwnerLoss};
use crate::ribbons::{spawn_ribbon, RibbonSeq};
use crate::rig_anim::{GlobalSeqDrive, RigPose};
use crate::rig_palette::{RigPalettes, RigPart, RigSkin};

/// The fade sphere the instance's emitters and ribbons are gated by. A lone model is always
/// "near", so this only needs to be larger than any sensible viewing distance.
const FX_FADE_RADIUS: f32 = 200.0;

/// Everything [`spawn_single_model`] needs to know about the one model it places.
pub struct SingleModelSpec<'a> {
    /// The loaded model asset.
    pub model: &'a M2Model,
    /// The model's app-built render forms, **complete**: the caller has seen
    /// [`crate::model_forms::ModelForms::require_rigged`] return `true`, or called
    /// [`crate::model_forms::ModelForms::ensure_now_rigged`].
    pub forms: FormSlices<'a>,
    /// `CreatureDisplayInfo.textureVariation` for the `Monster1/2/3` skin slots — bare names,
    /// resolved beside the model file.
    pub skins: &'a [Option<String>; 3],
    /// The model's path inside the chain — the instance's label for the hover inspector and
    /// `WOW_EMIT_DUMP`.
    pub model_path: &'a str,
    /// The display this instance stands for (the [`WorldObject`] id).
    pub display_id: u32,
    /// Where it stands, Bevy space; `scale` is the instance scale.
    pub transform: Transform,
    /// `AnimationData.dbc` id to loop from the first frame; `None` = the model's idle seed.
    pub start_anim: Option<u16>,
    /// Rig the model (skinned parts, palette slot, player-driven pose). `false` draws the static
    /// form at bind pose — a viewer's "show me the mesh" and the rig lane's own A/B control.
    pub rig: bool,
    /// A character-model NPC's **pre-baked body atlas**, as an `mpq://` URL
    /// (`CreatureDisplayInfoExtra`'s bake name under `Textures\BakedNpcTextures\`). A
    /// `CharSkinSlot::Body` batch carries texture type 1 — the M2 names no file, because the
    /// client supplies the body at runtime — so without this a humanoid NPC draws untextured.
    ///
    /// `None` for a beast model (which names its own textures) and for the live-composite case:
    /// a row with no bake name wants CharSections compositing, which is the game's `char_skin`
    /// and is deliberately not reimplemented here. Hair, cape and extra-skin slots are likewise
    /// left to whoever needs them — this is the body, the one slot a viewer cannot do without.
    pub body_atlas: Option<String>,
    /// For a character model: the geoset ids to draw
    /// ([`benilla_formats::CharacterGeosets::visible_geosets`]). A character M2 ships **every**
    /// variant of every group — every hairstyle, both ear shapes, six cloaks — and the client
    /// draws only what the appearance selects; a batch whose `geoset_id` is outside this set is
    /// skipped. `None` draws every batch, which is right for a creature/doodad model (they carry
    /// one geoset, id 0) and wrong for a character body.
    pub visible_geosets: Option<&'a [u16]>,
    /// A character body's **hair sheet** as an `mpq://` URL (M2 texture type 6, the CharSections
    /// row for the appearance's style + colour). The baked atlas covers the head *skin*, never the
    /// hair cards, so without this a styled head draws untextured. `None` for a bald style, and
    /// for every model with no hair batch.
    pub hair_texture: Option<String>,
}

/// What [`spawn_single_model`] spawned. `parts` are children of `root`; `fx` are world roots
/// riding the pose's anchors — despawn both lists (or `root` recursively plus `fx`) to remove the
/// instance.
pub struct SingleModel {
    /// The instance root: transform, [`WorldObject`], pose buffer, palette rig, player.
    pub root: Entity,
    /// One entity per drawn submesh.
    pub parts: Vec<Entity>,
    /// The instance's particle emitters and ribbon trails.
    pub fx: Vec<Entity>,
    /// The palette slot the skinned parts tag; `0` = drawn static (no joints, no skinned form,
    /// or the palette table was full).
    pub rig_slot: u16,
}

/// Spawn one model instance at `spec.transform`. `None` while the shared light buffer is not
/// up yet — the same retry-next-frame contract the entity lane has for a still-loading asset.
pub fn spawn_single_model(
    commands: &mut Commands,
    asset_server: &AssetServer,
    mats: &mut M2BatchMaterials<'_>,
    palettes: &mut RigPalettes,
    ibps: &Assets<SkinnedMeshInverseBindposes>,
    spec: SingleModelSpec<'_>,
) -> Option<SingleModel> {
    if !mats.ready() {
        return None;
    }
    let model = spec.model;
    let root = commands
        .spawn((
            spec.transform,
            Visibility::default(),
            WorldObject {
                kind: ModelKind::Creature,
                label: spec.model_path.to_owned(),
                id: spec.display_id,
                detail: String::new(),
            },
        ))
        .id();

    // The rig: a collapsed pose buffer (decision 1365 — no joint entities) plus an EAGER palette
    // slot, the booth's law rather than the doodad lane's lazy one.
    let has_joints = spec.rig && !model.skeleton.joints.is_empty();
    let mut pose = has_joints.then(|| RigPose::new(root, &model.skeleton));
    let rig_slot = match (&pose, spec.forms.skin) {
        (Some(pose), Some(_)) => RigSkin::allocate_bones(
            palettes,
            model.skeleton.joints.len() as u32,
            model.inverse_bindposes.clone(),
        )
        .map_or(0, |rig| {
            let slot = rig.slot;
            // Seed the slot's rows from the bind pose NOW -- the lazy lane's own rule (decision
            // 1365): until the pose pass has written them, an unseeded slot skins every vertex
            // to the origin and the first frames render nothing.
            if let Some(ibp) = ibps.get(&model.inverse_bindposes) {
                crate::rig_anim::seed_rig_rows(
                    pose,
                    GlobalTransform::from(spec.transform),
                    &rig,
                    &ibp[..],
                    palettes,
                );
            }
            commands.entity(root).insert(rig);
            slot
        }),
        _ => 0,
    };
    let use_rig = rig_slot != 0;

    let dir = model_dir(spec.model_path);
    let mut parts = Vec::with_capacity(model.submeshes.len());
    for (i, sub) in model.submeshes.iter().enumerate() {
        // The appearance's geoset selection, when the caller computed one (a character body).
        if spec
            .visible_geosets
            .is_some_and(|set| !set.contains(&sub.geoset_id))
        {
            continue;
        }
        let mesh = if use_rig {
            spec.forms.skin.and_then(|s| s.get(i)).cloned()
        } else {
            spec.forms.stat.get(i).map(|(h, _)| h.clone())
        };
        let Some(mesh) = mesh else { continue };
        let texture = resolve_skin(
            sub,
            dir,
            spec.skins,
            spec.body_atlas.as_deref(),
            spec.hair_texture.as_deref(),
            asset_server,
        );
        let order = u16::try_from(i + 1).unwrap_or(u16::MAX);
        debug!(
            "single_model: batch {i} geoset {} tex {} char_slot {:?} skin_slot {:?} blend {:?} billboard {} two_sided {} aabb {:?}",
            sub.geoset_id,
            match (&sub.texture, sub.char_slot, texture.is_some()) {
                (Some(_), _, _) => "authored",
                (None, Some(CharSkinSlot::Body), true) => "body-atlas",
                (None, Some(CharSkinSlot::Hair), true) => "hair-sheet",
                (None, _, true) => "skin",
                (None, _, false) => "NONE",
            },
            sub.char_slot,
            sub.skin_slot,
            sub.blend,
            sub.billboard.is_some(),
            sub.two_sided,
            spec.forms.stat.get(i).and_then(|(_, a)| a.as_ref()).map(|a| a.half_extents)
        );
        let Some(material) = mats.steady(sub, texture, order) else {
            continue;
        };
        let mut part = commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::IDENTITY,
            ChildOf(root),
            ModelPart {
                kind: ModelKind::Creature,
                blend: sub.blend,
            },
            MeshTag(spawn_tag(if use_rig { rig_slot } else { 0 }, 1.0)),
        ));
        if use_rig {
            // A skinned part's only Bevy bound is its bind-pose box, which the palette leaves
            // behind — the booth's law (decision 1577): never frustum-cull a part the GPU poses.
            part.insert((RigPart(root), NoFrustumCulling));
        } else if let Some((_, Some(aabb))) = spec.forms.stat.get(i) {
            // The static form is a RENDER_WORLD-only mesh: Bevy cannot compute its bound, so the
            // build-time one goes on explicitly — the placement lane's rule (decision 0834).
            part.insert((*aabb, NoAutoAabb));
        }
        parts.push(part.id());
    }

    // The player goes up whenever the model has clips, rig or not: a boneless model with
    // emitters still needs the clock its rate tracks are sampled against (`EmitClock::Host`).
    let mut has_player = false;
    if let Some(anims) = &model.animations {
        if let Some(drive) =
            GlobalSeqDrive::new_rig(&anims.global_bones, model.skeleton.joints.len())
        {
            commands.entity(root).insert(drive);
        }
        let clip = spec
            .start_anim
            .and_then(|id| anims.find(id))
            .or_else(|| anims.idle_clip());
        let mut player = AnimationPlayer::default();
        if let Some(clip) = clip {
            player.play(clip.node).repeat();
        }
        commands.entity(root).insert((
            player,
            AnimationGraphHandle(anims.graph.clone()),
            anims.clone(),
        ));
        has_player = true;
    }

    // Emitters and ribbons ride the pose's anchors, minted here before the buffer is attached.
    let mut fx = Vec::new();
    if let Some(pose) = pose.as_mut() {
        let fade = EmitterFade::sphere(FX_FADE_RADIUS, spec.transform.translation);
        for em in &model.emitters {
            let anchor = pose.anchor_for(commands, root, em.def.bone);
            let frames = EmitterFrames {
                owner: anchor.map(|a| (a, em.bone_pivot)),
                anchor: None,
                on_owner_loss: OwnerLoss::Free,
                ..default()
            };
            let clock = if has_player {
                EmitClock::Host(root)
            } else {
                EmitClock::Pinned
            };
            if let Some(e) = spawn_emitter(commands, em, spec.transform, frames, clock) {
                commands.entity(e).insert(fade.clone());
                fx.push(e);
            }
        }
        let scale = spec.transform.scale.max_element();
        for rb in &model.ribbons {
            let (owner, use_pivot) = match pose.anchor_for(commands, root, rb.def.bone) {
                Some(a) => (a, true),
                None => (root, false),
            };
            let seq = if has_player {
                RibbonSeq::Host(root)
            } else {
                RibbonSeq::Fixed(0)
            };
            if let Some(e) = spawn_ribbon(
                commands,
                rb,
                owner,
                use_pivot,
                scale,
                seq,
                None,
                Some(fade.clone()),
            ) {
                fx.push(e);
            }
        }
    }
    if let Some(pose) = pose {
        commands.entity(root).insert(pose);
    }

    Some(SingleModel {
        root,
        parts,
        fx,
        rig_slot,
    })
}

/// Switch the instance's looping clip to `anim_id` (an `AnimationData.dbc` id). `false` when the
/// model has no sequence with that id — the player is left as it was.
pub fn play_sequence(player: &mut AnimationPlayer, anims: &ModelAnimations, anim_id: u16) -> bool {
    let Some(clip) = anims.find(anim_id) else {
        return false;
    };
    player.stop_all();
    player.play(clip.node).repeat();
    true
}

/// A batch's texture: the one the M2 names, else a character body atlas for a
/// [`CharSkinSlot::Body`] batch, else the display's variation for its skin slot, loaded beside the
/// model — the entity lane's own rule, plus the one runtime slot a viewer cannot skip.
fn resolve_skin(
    sub: &ModelSubmesh,
    dir: &str,
    skins: &[Option<String>; 3],
    body_atlas: Option<&str>,
    hair_texture: Option<&str>,
    asset_server: &AssetServer,
) -> Option<Handle<Image>> {
    // Texture types 1 and 6 name no file in the M2 — the client supplies the body and the hair at
    // runtime. For a character-model NPC the body is the shipped pre-baked atlas (loaded whole,
    // never composited) and the hair is its CharSections sheet.
    match (sub.char_slot, body_atlas, hair_texture) {
        (Some(CharSkinSlot::Body), Some(url), _) | (Some(CharSkinSlot::Hair), _, Some(url)) => {
            return Some(asset_server.load(url.to_owned()));
        }
        _ => {}
    }
    match (&sub.texture, sub.skin_slot) {
        (Some(t), _) => Some(t.clone()),
        (None, Some(slot)) => skins
            .get(slot as usize)
            .and_then(|o| o.as_ref())
            .map(|name| asset_server.load(skin_url(dir, name))),
        (None, None) => None,
    }
}

fn model_dir(model_path: &str) -> &str {
    model_path
        .rsplit_once(['\\', '/'])
        .map_or("", |(dir, _)| dir)
}
