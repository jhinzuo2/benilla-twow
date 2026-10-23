//! **The memory census**: every 30 s on a touch device, two `inilla: mem …` lines on stdout, so the
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

fn mb(bytes: u64) -> f64 {
    bytes as f64 / 1_048_576.0
}
