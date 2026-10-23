//! **The memory census**: every 30 s on a touch device, one `inilla: mem …` line on stdout, so the
//! live log, the log file and the remote console carry it. It splits what the process holds into
//! what benilla can count: the textures in `Assets<Image>` (block-compressed or not, the largest by
//! name) and the retained pass's texture arrays, beside the headroom iOS reports. On an iPhone the
//! GPU shares RAM, so every texture counts against the app's budget.

use bevy::prelude::*;

/// How many of the largest textures the line names.
const LARGEST: usize = 5;

/// Seconds between lines.
const EVERY: f64 = 30.0;

pub(crate) fn census(
    images: Res<Assets<Image>>,
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
    let mut sized: Vec<(u64, AssetId<Image>)> = Vec::with_capacity(images.len());
    for (id, image) in images.iter() {
        let bytes = benilla_assets::image_gpu_bytes(image) as u64;
        if image.texture_descriptor.format.is_compressed() {
            compressed += bytes;
        } else {
            plain += bytes;
        }
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
    let headroom = inilla_platform::device::available_memory()
        .map_or_else(|| "?".to_string(), |b| format!("{:.0}", mb(b)));
    println!(
        "inilla: mem headroom {headroom} MB | textures {}: {:.0} MB block-compressed, {:.0} MB \
         uncompressed (DXT upload {}) | world texture arrays {:.0} MB | largest: {}",
        sized.len(),
        mb(compressed),
        mb(plain),
        if benilla_assets::bc_supported() {
            "on"
        } else {
            "off"
        },
        mb(benilla_world::static_gx::pool_vram_bytes()),
        largest.join(", ")
    );
}

fn mb(bytes: u64) -> f64 {
    bytes as f64 / 1_048_576.0
}
