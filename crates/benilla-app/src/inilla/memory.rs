//! **The memory census**: every 30 s on a touch device, three `inilla: mem` lines on stdout, so the
//! live log, the log file and the remote console carry them. They split what the process holds
//! into what benilla can count, beside the headroom iOS reports: every Metal allocation (textures,
//! buffers, render targets), the textures in `Assets<Image>` (block-compressed or not, pixels still
//! kept on the CPU, the largest by name), the retained pass's texture arrays, meshes whose vertex
//! data is still on the CPU, the loaded model and terrain assets, and the decoded SFX cache. On an
//! iPhone the GPU shares RAM, so all of it counts against the app's budget.

use bevy::mesh::{Indices, Mesh};
use bevy::prelude::*;

use benilla_assets::{AdtTile, M2Model, WmoModel};

/// How many of the largest textures the line names.
const LARGEST: usize = 5;

/// Seconds between lines.
const EVERY: f64 = 30.0;

#[allow(clippy::too_many_arguments)]
pub(crate) fn census(
    images: Res<Assets<Image>>,
    meshes: Res<Assets<Mesh>>,
    m2s: Option<Res<Assets<M2Model>>>,
    wmos: Option<Res<Assets<WmoModel>>>,
    adts: Option<Res<Assets<AdtTile>>>,
    kits: Option<Res<crate::sound::SoundKits>>,
    server: Res<AssetServer>,
    time: Res<Time<Real>>,
    mut next: Local<f64>,
) {
    let now = time.elapsed_secs_f64();
    if now < *next || !super::touch_device() {
        return;
    }
    *next = now + EVERY;

    let mut compressed = 0u64;
    let mut plain = 0u64;
    // Pixels still held main-side after upload: a texture whose usage keeps `MAIN_WORLD` is in
    // memory twice.
    let mut kept = 0u64;
    let mut sized: Vec<(u64, AssetId<Image>)> = Vec::with_capacity(images.len());
    for (id, image) in images.iter() {
        let bytes = benilla_assets::image_gpu_bytes(image) as u64;
        if image.texture_descriptor.format.is_compressed() {
            compressed += bytes;
        } else {
            plain += bytes;
        }
        kept += image.data.as_ref().map_or(0, |d| d.len() as u64);
        sized.push((bytes, id));
    }
    sized.sort_unstable_by_key(|&(bytes, _)| std::cmp::Reverse(bytes));
    let largest: Vec<String> = sized
        .iter()
        .take(LARGEST)
        .map(|&(bytes, id)| {
            let name = server
                .get_path(id)
                .map_or_else(|| "(generated)".to_string(), |p| p.to_string());
            format!("{name} {:.1}", mb(bytes))
        })
        .collect();

    // A mesh whose data went to the render world answers `Err`; one that kept `MAIN_WORLD` still
    // holds its vertices and indices here, beside the GPU copy.
    let mut mesh_kept = 0usize;
    let mut mesh_bytes = 0u64;
    for (_, mesh) in meshes.iter() {
        let Ok(attributes) = mesh.try_attributes() else {
            continue;
        };
        mesh_kept += 1;
        mesh_bytes += attributes
            .map(|(attribute, values)| values.len() as u64 * attribute.format.size())
            .sum::<u64>();
        mesh_bytes += match mesh.try_indices_option() {
            Ok(Some(Indices::U16(i))) => i.len() as u64 * 2,
            Ok(Some(Indices::U32(i))) => i.len() as u64 * 4,
            _ => 0,
        };
    }
    let (sfx, sfx_bytes) = kits.as_ref().map_or((0, 0), |k| k.cache_size());

    let headroom = inilla_platform::device::available_memory()
        .map_or_else(|| "?".to_string(), |b| format!("{:.0}", mb(b)));
    let gpu = inilla_platform::device::gpu_allocated()
        .map_or_else(|| "?".to_string(), |b| format!("{:.0}", mb(b)));
    println!(
        "inilla: mem headroom {headroom} MB | Metal {gpu} MB | textures {}: {:.0} MB \
         block-compressed, {:.0} MB uncompressed (DXT upload {}), {:.0} MB of pixels also kept on \
         the CPU | world texture arrays {:.0} MB",
        sized.len(),
        mb(compressed),
        mb(plain),
        if benilla_assets::bc_supported() {
            "on"
        } else {
            "off"
        },
        mb(kept),
        mb(benilla_world::static_gx::pool_vram_bytes()),
    );
    println!(
        "inilla: mem meshes {} ({} with CPU data, {:.0} MB) | models {} M2, {} WMO, {} ADT | \
         decoded SFX {sfx} ({:.0} MB) | largest textures: {}",
        meshes.len(),
        mesh_kept,
        mb(mesh_bytes),
        m2s.as_ref().map_or(0, |a| a.len()),
        wmos.as_ref().map_or(0, |a| a.len()),
        adts.as_ref().map_or(0, |a| a.len()),
        mb(sfx_bytes),
        largest.join(", ")
    );
}

/// Once, at the first look: where BC is lost, if it is. The GPU's own answer, the adapter wgpu
/// built from it, and the device Bevy opened.
pub(crate) fn report_gpu_caps(
    adapter: Option<Res<bevy::render::renderer::RenderAdapter>>,
    device: Option<Res<bevy::render::renderer::RenderDevice>>,
    mut done: Local<bool>,
) {
    if *done || !super::touch_device() {
        return;
    }
    *done = true;
    let bc = bevy::render::settings::WgpuFeatures::TEXTURE_COMPRESSION_BC;
    println!(
        "inilla: gpu BC: Metal says {:?}, adapter {:?}, device {:?}",
        inilla_platform::device::bc_texture_compression(),
        adapter.map(|a| a.features().contains(bc)),
        device.map(|d| d.features().contains(bc)),
    );
}

/// The third line: what the heap holds that the first two cannot see. The malloc total against
/// the footprint, then the CPU copies the world keeps: model geometry and collision data, the
/// physics colliders' triangles, the animation assets and the entity count.
#[allow(clippy::too_many_arguments)]
pub(crate) fn census_cpu(
    m2s: Option<Res<Assets<M2Model>>>,
    wmos: Option<Res<Assets<WmoModel>>>,
    clips: Option<Res<Assets<AnimationClip>>>,
    graphs: Option<Res<Assets<AnimationGraph>>>,
    colliders: Query<&avian3d::prelude::Collider>,
    entities: Query<Entity>,
    time: Res<Time<Real>>,
    mut next: Local<f64>,
) {
    let now = time.elapsed_secs_f64();
    if now < *next || !super::touch_device() {
        return;
    }
    *next = now + EVERY;

    // A submesh's geometry is shared (`Arc`) across the models that load it: counted once.
    let mut seen = std::collections::HashSet::<*const ()>::new();
    let mut geometry = 0u64;
    let mut collision = 0u64;
    let mut add_submeshes = |submeshes: &[benilla_assets::ModelSubmesh], geometry: &mut u64| {
        for sub in submeshes {
            if !seen.insert(std::sync::Arc::as_ptr(&sub.geometry).cast()) {
                continue;
            }
            let g = &sub.geometry;
            *geometry += (g.positions.len() * 12
                + g.normals.len() * 12
                + g.uvs.len() * 8
                + g.indices.len() * 4
                + g.joints.len() * 8
                + g.weights.len() * 16
                + g.vertex_colors.len() * 16) as u64;
        }
    };
    if let Some(m2s) = &m2s {
        for (_, m2) in m2s.iter() {
            add_submeshes(&m2.submeshes, &mut geometry);
            collision += m2.collision.as_ref().map_or(0, collision_mesh_bytes);
        }
    }
    if let Some(wmos) = &wmos {
        for (_, wmo) in wmos.iter() {
            add_submeshes(&wmo.submeshes, &mut geometry);
            collision += wmo.collision.as_ref().map_or(0, collision_mesh_bytes);
            // Triangle soup: three points of three `f32`s each.
            let soup: usize = wmo
                .group_collision_tris
                .iter()
                .chain(&wmo.group_camera_only_tris)
                .map(Vec::len)
                .sum();
            collision += (soup * 36 + wmo.portal_vertices.len() * 12) as u64;
        }
    }

    // A trimesh holds its vertices and triangles, plus a BVH this does not size.
    let mut trimeshes = 0usize;
    let mut tri_bytes = 0u64;
    for collider in &colliders {
        if let Some(t) = collider.shape_scaled().as_trimesh() {
            trimeshes += 1;
            tri_bytes += (t.vertices().len() * 12 + t.indices().len() * 12) as u64;
        }
    }

    let heap = inilla_platform::device::malloc_in_use()
        .map_or_else(|| "?".to_string(), |b| format!("{:.0}", mb(b)));
    println!(
        "inilla: mem cpu malloc {heap} MB | model geometry {:.0} MB, model collision {:.0} MB | \
         colliders {} ({trimeshes} trimesh, {:.0} MB + BVH) | animation clips {}, graphs {}, pose \
         keyframes {:.0} MB (the clips' curves hold about as much again) | entities {}",
        mb(geometry),
        mb(collision),
        colliders.iter().count(),
        mb(tri_bytes),
        clips.as_ref().map_or(0, |a| a.len()),
        graphs.as_ref().map_or(0, |a| a.len()),
        mb(benilla_assets::pose_key_bytes()),
        entities.iter().count(),
    );
}

/// One line at every client-state change (login, character select, world): the malloc total and
/// the headroom at that moment, so the census can tell what boot costs from what the world adds.
pub(crate) fn phase_snapshot(state: Res<State<crate::client_state::ClientState>>) {
    if !state.is_changed() || !super::touch_device() {
        return;
    }
    let show = |b: Option<u64>| b.map_or_else(|| "?".to_string(), |b| format!("{:.0}", mb(b)));
    println!(
        "inilla: mem at {:?}: malloc {} MB, Metal {} MB, headroom {} MB",
        state.get(),
        show(inilla_platform::device::malloc_in_use()),
        show(inilla_platform::device::gpu_allocated()),
        show(inilla_platform::device::available_memory()),
    );
}

fn collision_mesh_bytes(m: &benilla_formats::CollisionMesh) -> u64 {
    (m.positions.len() * 12 + m.indices.len() * 4) as u64
}

fn mb(bytes: u64) -> f64 {
    bytes as f64 / 1_048_576.0
}
