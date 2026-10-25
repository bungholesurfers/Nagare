//! The update manifest and its signature, shared by the app's updater (`update.rs`) and the release tool
//! (`src/bin/release.rs`), which writes and signs the manifest in the release workflow.
//!
//! The manifest is a small JSON file, `nagare-update.json`, published with each release: the version, the
//! installer's file name, its size and its SHA-256. `nagare-update.json.sig` beside it is an Ed25519 signature of the
//! manifest's exact bytes, in hex, made with the key the workflow holds as the secret NAGARE_UPDATE_KEY. The updater
//! believes a manifest only if one of PUBLIC_KEYS verifies it, so neither GitHub nor anything between it and the user
//! can hand Nagare an installer the key's holder did not publish. Several keys may be listed, to change keys without
//! stranding the copies that know only the old one.

#![cfg_attr(not(windows), allow(dead_code))]

/// The manifest's file name in a release, and its signature's.
pub const MANIFEST: &str = "nagare-update.json";
pub const SIGNATURE: &str = "nagare-update.json.sig";

/// The keys a manifest may be signed with, as hex: the public halves of the release workflow's NAGARE_UPDATE_KEY.
pub const PUBLIC_KEYS: &[&str] = &["2a30f62f6a2355271e47e71338415c64bb578a7aa20d6cbfe343aa8e4cf22c07"];

/// What a manifest says.
#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    pub version: String,
    pub installer: String,
    pub size: u64,
    pub sha256: String,
}

/// The installer's file name for `version`, the only one a manifest may name.
pub fn installer_name(version: &str) -> String {
    format!("nagare-setup-{version}.exe")
}

/// The manifest's bytes, as the release tool writes and signs them.
#[allow(dead_code)] // the release tool's, and the tests'
pub fn manifest_json(m: &Manifest) -> String {
    serde_json::json!({
        "version": m.version,
        "installer": m.installer,
        "size": m.size,
        "sha256": m.sha256,
    })
    .to_string()
}

/// The manifest in `bytes`, if one of `keys` (hex) verifies `signature` (hex) over exactly those bytes, and it is
/// well formed: a plain version, the installer named for it, a SHA-256 in hex.
#[cfg(windows)]
pub fn verify(bytes: &[u8], signature: &str, keys: &[&str]) -> Result<Manifest, String> {
    use ed25519_dalek::{Signature, VerifyingKey};
    let sig: [u8; 64] = unhex(signature.trim()).and_then(|b| b.try_into().ok()).ok_or("the signature is not 64 bytes of hex")?;
    let sig = Signature::from_bytes(&sig);
    let trusted = keys.iter().any(|k| {
        unhex(k)
            .and_then(|b| <[u8; 32]>::try_from(b).ok())
            .and_then(|b| VerifyingKey::from_bytes(&b).ok())
            .is_some_and(|key| key.verify_strict(bytes, &sig).is_ok())
    });
    if !trusted {
        return Err("the update's signature does not match Nagare's key".into());
    }
    let v: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| format!("the manifest is not JSON: {e}"))?;
    let text = |k: &str| v[k].as_str().map(str::to_string).ok_or(format!("the manifest has no {k}"));
    let m = Manifest {
        version: text("version")?,
        installer: text("installer")?,
        size: v["size"].as_u64().ok_or("the manifest has no size")?,
        sha256: text("sha256")?.to_lowercase(),
    };
    if crate::version::parse(&m.version).is_none() {
        return Err(format!("the manifest's version {:?} is not a version", m.version));
    }
    if m.installer != installer_name(&m.version) {
        return Err(format!("the manifest names an unexpected installer, {:?}", m.installer));
    }
    if m.sha256.len() != 64 || unhex(&m.sha256).is_none() {
        return Err("the manifest's SHA-256 is not 32 bytes of hex".into());
    }
    Ok(m)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok()).collect()
}
