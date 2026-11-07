// Embeds the application manifest (DPI awareness, visual styles) and an icon drawn by the same
// rasterizer the app uses at runtime, so the .exe in Explorer matches the tray icon.

#[cfg(windows)]
#[path = "src/icon_art.rs"]
#[allow(dead_code)]
mod icon_art;

#[cfg(windows)]
use std::fmt::Write as _;

/// A 32-bit BMP-based .ico containing the artwork at several sizes.
#[cfg(windows)]
fn make_ico(sizes: &[i32]) -> Vec<u8> {
    let mut images: Vec<Vec<u8>> = Vec::new();
    for &size in sizes {
        let rgba = icon_art::icon_pixels(size);
        let mut data = Vec::new();
        // BITMAPINFOHEADER; the height counts the colour bitmap and the AND mask
        data.extend_from_slice(&40u32.to_le_bytes());
        data.extend_from_slice(&(size as u32).to_le_bytes());
        data.extend_from_slice(&((size * 2) as u32).to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&32u16.to_le_bytes());
        data.extend_from_slice(&[0u8; 24]);
        // pixels, bottom-up BGRA
        for y in (0..size).rev() {
            for x in 0..size {
                let i = ((y * size + x) * 4) as usize;
                data.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
            }
        }
        // AND mask: all zero (transparency comes from the alpha channel)
        let mask_row = (((size + 31) / 32) * 4) as usize;
        data.extend(std::iter::repeat(0u8).take(mask_row * size as usize));
        images.push(data);
    }

    let mut ico = Vec::new();
    ico.extend_from_slice(&0u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len();
    for (size, data) in sizes.iter().zip(&images) {
        ico.push(*size as u8);
        ico.push(*size as u8);
        ico.extend_from_slice(&[0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&(data.len() as u32).to_le_bytes());
        ico.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += data.len();
    }
    for data in images {
        ico.extend_from_slice(&data);
    }
    ico
}

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
fn main() {
    println!("cargo:rerun-if-changed=margin.manifest");
    println!("cargo:rerun-if-changed=src/icon_art.rs");
    println!("cargo:rerun-if-changed=build.rs");

    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let ico_path = out.join("margin.ico");
    std::fs::write(&ico_path, make_ico(&[16, 24, 32, 48, 64])).expect("write margin.ico");

    // rc.exe wants forward slashes (or doubled backslashes) in resource paths
    let forward = |p: &std::path::Path| p.to_string_lossy().replace('\\', "/");
    let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir")).join("margin.manifest");
    let mut rc = String::new();
    writeln!(rc, "1 ICON \"{}\"", forward(&ico_path)).unwrap();
    writeln!(rc, "1 24 \"{}\"", forward(&manifest)).unwrap(); // RT_MANIFEST = 24

    // version info shown in Explorer's Details tab and Task Manager
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let mut parts: Vec<String> = version.split('.').map(str::to_string).collect();
    parts.resize(4, "0".into());
    writeln!(
        rc,
        "1 VERSIONINFO
FILEVERSION {v}
PRODUCTVERSION {v}
FILEOS 0x4
FILETYPE 0x1
{{
         BLOCK \"StringFileInfo\" {{ BLOCK \"040904b0\" {{
         VALUE \"FileDescription\", \"Margin - touchpad edges and corners\"
         VALUE \"ProductName\", \"Margin\"
         VALUE \"FileVersion\", \"{version}\"
         VALUE \"ProductVersion\", \"{version}\"
         VALUE \"OriginalFilename\", \"margin.exe\"
         VALUE \"LegalCopyright\", \"MIT License\"
         }} }}
         BLOCK \"VarFileInfo\" {{ VALUE \"Translation\", 0x409, 1200 }}
}}",
        v = parts.join(",")
    )
    .unwrap();
    let rc_path = out.join("margin.rc");
    std::fs::write(&rc_path, rc).expect("write margin.rc");

    embed_resource::compile(&rc_path, embed_resource::NONE);
}
