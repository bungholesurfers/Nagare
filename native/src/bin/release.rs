//! nagare-release: the release workflow's helper for the updater's manifest (`signing.rs`).
//!
//!   nagare-release keygen SECRET_FILE
//!       Makes a new Ed25519 key: writes its secret half, in hex, to SECRET_FILE (which must not exist), and prints
//!       its public half, for signing.rs's PUBLIC_KEYS. The secret goes into the repository's secret NAGARE_UPDATE_KEY
//!       and somewhere safe; whoever holds it can publish updates every copy of Nagare will install.
//!
//!   nagare-release manifest INSTALLER VERSION OUT_DIR
//!       Writes OUT_DIR\nagare-update.json for INSTALLER, which must be named for VERSION, and signs it into
//!       OUT_DIR\nagare-update.json.sig with the secret in the environment variable NAGARE_UPDATE_KEY. It checks the
//!       signature with signing.rs's PUBLIC_KEYS before it is done, so a key the app does not know fails the release.

#[path = "../signing.rs"]
mod signing;
// The app's own version checks, of which the tool needs only parse().
#[path = "../version.rs"]
#[cfg_attr(windows, allow(dead_code))]
mod version;

#[cfg(windows)]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["keygen", file] => keygen(file),
        ["manifest", installer, version, out] => manifest(installer, version, out),
        _ => Err("usage: nagare-release keygen SECRET_FILE | manifest INSTALLER VERSION OUT_DIR".into()),
    };
    if let Err(e) = result {
        eprintln!("nagare-release: {e}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("nagare-release: Windows only, as the releases are");
    std::process::exit(1);
}

#[cfg(windows)]
fn keygen(file: &str) -> Result<(), String> {
    use std::io::Write;
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).map_err(|e| format!("no randomness: {e}"))?;
    let key = ed25519_dalek::SigningKey::from_bytes(&seed);
    // create_new: never overwrite a key that may already sign releases.
    let mut out = std::fs::OpenOptions::new().write(true).create_new(true).open(file).map_err(|e| format!("{file}: {e}"))?;
    writeln!(out, "{}", signing::hex(&key.to_bytes())).map_err(|e| format!("{file}: {e}"))?;
    println!("{}", signing::hex(&key.verifying_key().to_bytes()));
    Ok(())
}

#[cfg(windows)]
fn manifest(installer: &str, version: &str, out: &str) -> Result<(), String> {
    use ed25519_dalek::Signer;
    use sha2::{Digest, Sha256};
    let version = version.strip_prefix('v').unwrap_or(version);
    if version::parse(version).is_none() {
        return Err(format!("{version:?} is not a version"));
    }
    let path = std::path::Path::new(installer);
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    if name != signing::installer_name(version) {
        return Err(format!("the installer {name:?} is not named for {version}"));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{installer}: {e}"))?;
    let m = signing::Manifest {
        version: version.to_string(),
        installer: name.to_string(),
        size: bytes.len() as u64,
        sha256: signing::hex(&Sha256::digest(&bytes)),
    };
    let json = signing::manifest_json(&m);

    let secret = std::env::var("NAGARE_UPDATE_KEY").map_err(|_| "NAGARE_UPDATE_KEY is not set")?;
    let secret: [u8; 32] = signing::unhex(secret.trim()).and_then(|b| b.try_into().ok()).ok_or("NAGARE_UPDATE_KEY is not 32 bytes of hex")?;
    let key = ed25519_dalek::SigningKey::from_bytes(&secret);
    let signature = signing::hex(&key.sign(json.as_bytes()).to_bytes());
    // The app must believe it: the key's public half has to be among the ones it knows.
    signing::verify(json.as_bytes(), &signature, signing::PUBLIC_KEYS)
        .map_err(|e| format!("{e} (is the key's public half, {}, in signing.rs?)", signing::hex(&key.verifying_key().to_bytes())))?;

    let out = std::path::Path::new(out);
    std::fs::write(out.join(signing::MANIFEST), &json).map_err(|e| e.to_string())?;
    std::fs::write(out.join(signing::SIGNATURE), &signature).map_err(|e| e.to_string())?;
    println!("{json}");
    Ok(())
}
