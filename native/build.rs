//! Puts the logo's icon (`assets/logo/nagare.ico`, which `nagare-customize` renders) into the Windows executables, for
//! Explorer, the taskbar and Alt+Tab. The icon goes in as a compiled resource file, written here - each of the .ico's
//! images an RT_ICON, and an RT_GROUP_ICON listing them - which the MSVC linker takes as it is, so that no resource
//! compiler (rc.exe, from the Windows SDK) is needed.

use std::path::PathBuf;

const ICO: &str = "assets/logo/nagare.ico";
const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const LANGUAGE: u16 = 0x0409;

/// A resource's header, its type and name numbers, then its data, padded to four bytes.
fn resource(out: &mut Vec<u8>, kind: u16, name: u16, flags: u16, data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes());
    out.extend_from_slice(&[0xff, 0xff]);
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(&[0xff, 0xff]);
    out.extend_from_slice(&name.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&LANGUAGE.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(data);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

fn main() {
    println!("cargo:rerun-if-changed={ICO}");
    let windows = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|env| env == "msvc");
    if !windows || !msvc {
        return;
    }
    let Ok(ico) = std::fs::read(ICO) else {
        println!("cargo:warning=no {ICO}: the executables get no icon (run nagare-customize --render)");
        return;
    };
    let u16_at = |i: usize| u16::from_le_bytes([ico[i], ico[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([ico[i], ico[i + 1], ico[i + 2], ico[i + 3]]);
    let count = u16_at(4) as usize;
    // The empty resource every .res file begins with.
    let mut res = Vec::new();
    resource(&mut res, 0, 0, 0, &[]);
    let mut group = Vec::new();
    group.extend_from_slice(&0u16.to_le_bytes());
    group.extend_from_slice(&1u16.to_le_bytes());
    group.extend_from_slice(&(count as u16).to_le_bytes());
    for i in 0..count {
        let entry = 6 + 16 * i;
        let (size, offset) = (u32_at(entry + 8) as usize, u32_at(entry + 12) as usize);
        let id = i as u16 + 1;
        resource(&mut res, RT_ICON, id, 0x1010, &ico[offset..offset + size]);
        // The group's entry: the .ico's, its offset replaced by the image's resource number.
        group.extend_from_slice(&ico[entry..entry + 12]);
        group.extend_from_slice(&id.to_le_bytes());
    }
    resource(&mut res, RT_GROUP_ICON, 1, 0x1030, &group);
    let path = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("nagare.res");
    std::fs::write(&path, res).expect("write the icon's resource file");
    println!("cargo:rustc-link-arg-bins={}", path.display());
}
