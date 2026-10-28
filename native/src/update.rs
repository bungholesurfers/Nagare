//! Software Update, Windows only: whether a newer Nagare is out, and, for a copy the installer installed, fetching
//! that version's installer and running it silently - which stops Nagare, installs over it and starts it again.
//!
//! What is stable enough to update to is the tag in the repository's `update/stable.txt`; on updateChannel's Latest
//! release, it is the newest release published instead, its tag read off where GitHub's latest-release link redirects
//! (a HEAD request: no page is fetched). Each release carries a
//! manifest of its installer, signed with the release key (`signing.rs`). A check reads the tag, and, when it names a
//! version newer than this one, that release's manifest and signature: the manifest is believed only if the key
//! signed it, names that very version and an installer named for it, and the installer is kept only if its size and
//! SHA-256 are the manifest's. A tag naming an older version, or this one, does nothing, so the unsigned tag can at
//! worst hold updates back; the latest release's tag no more than offers a release the key signed.
//!
//! Every request goes to GitHub alone, over HTTPS alone - each redirect checked too, so nothing ever falls back to
//! plain HTTP, as HSTS has a browser do - with TLS checked against Mozilla's roots, bundled, rather than whatever roots
//! the system holds, timeouts, and a cap on every body's size. The certificates are not pinned: GitHub changes them
//! and their authorities without notice, and a pinned updater stops updating, the fix included; the signature is what
//! makes a download trustworthy, whatever happened to the connection. Nothing is sent but the requests themselves,
//! with a User-Agent naming Nagare and its version.
//!
//! The installer is downloaded into %TEMP%\nagare-update, as VS Code's user setup downloads its own into its folder
//! there. The network is only ever touched from the updater's own thread, and only when `main.rs` asks: after the user
//! has said yes (or closed the question), or pressed Check now.
//!
//! NAGARE_UPDATE_SOURCE=FOLDER reads the files from FOLDER instead of GitHub (stable.txt, latest.txt standing for the
//! latest release's tag, the manifest, its signature and the installer, side by side), to try the whole of it without publishing anything.

#![cfg_attr(not(windows), allow(dead_code))]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[cfg(windows)]
use crate::signing;
#[cfg(windows)]
use crate::version;

/// Where the releases live.
pub const REPO: &str = "bungholesurfers/Nagare";
/// The releases' page, for a copy that cannot install an update itself.
pub const RELEASES_PAGE: &str = "https://github.com/bungholesurfers/Nagare/releases/latest";
/// The release page of `version`: its notes, and its files.
pub fn release_page(version: &str) -> String {
    format!("https://github.com/{REPO}/releases/tag/v{version}")
}

/// Opens `url` in the system's browser.
#[cfg(windows)]
pub fn open_page(url: &str) {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (verb, url) = (wide("open"), wide(url));
    unsafe {
        ShellExecuteW(core::ptr::null_mut(), verb.as_ptr(), url.as_ptr(), core::ptr::null(), core::ptr::null(), SW_SHOWNORMAL);
    }
}

#[cfg(not(windows))]
pub fn open_page(_url: &str) {}

/// Where the newest release published (not a draft, not a pre-release) is: GitHub redirects it to that release's page,
/// whose address ends in its tag.
const LATEST_URL: &str = RELEASES_PAGE;
/// The tag of the release to update to.
const STABLE_URL: &str = "https://raw.githubusercontent.com/bungholesurfers/Nagare/main/update/stable.txt";
/// The only hosts a request, or a redirect, may go to: the repository's files, its releases, and the storage GitHub
/// redirects release downloads to.
const HOSTS: [&str; 4] = ["github.com", "raw.githubusercontent.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com"];
/// The most each file may be: the tag, the manifest, its signature, the installer.
const MAX_TAG: u64 = 64;
const MAX_MANIFEST: u64 = 4096;
const MAX_SIGNATURE: u64 = 256;
const MAX_INSTALLER: u64 = 256 * 1024 * 1024;

/// What the updater is doing, or last did.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Idle,
    Checking,
    /// The tag checked (the stable one, or the latest release's) is this version, or an older one.
    UpToDate { stable: String },
    /// A newer version is out, which this copy cannot install itself (it was not installed by the installer).
    Available { version: String },
    Downloading { version: String, done: u64, total: u64 },
    /// The installer is downloaded and checked, at `path`, waiting to be run.
    Ready { version: String, path: PathBuf },
    /// The installer runs; it stops Nagare.
    Installing { version: String },
    /// This version's installer ran before and Nagare is still older: an automatic check leaves it be, rather than
    /// install it again and again, and points at the releases' page.
    Stuck { version: String },
    Failed(String),
}

impl Status {
    /// Whether the updater's thread is at work.
    pub fn busy(&self) -> bool {
        matches!(self, Status::Checking | Status::Downloading { .. })
    }

    /// Whether a check has finished, and how: Some(true) for an answer, Some(false) for a failure.
    pub fn finished(&self) -> Option<bool> {
        match self {
            Status::UpToDate { .. } | Status::Available { .. } | Status::Ready { .. } | Status::Installing { .. } | Status::Stuck { .. } => {
                Some(true)
            }
            Status::Failed(_) => Some(false),
            Status::Idle | Status::Checking | Status::Downloading { .. } => None,
        }
    }
}

/// The updater: its status, which its thread writes.
pub struct Updater {
    status: Arc<Mutex<Status>>,
}

impl Updater {
    pub fn new() -> Self {
        Updater { status: Arc::new(Mutex::new(Status::Idle)) }
    }

    pub fn status(&self) -> Status {
        self.status.lock().map(|s| s.clone()).unwrap_or(Status::Idle)
    }

    pub fn set(&self, status: Status) {
        if let Ok(mut s) = self.status.lock() {
            *s = status;
        }
    }

    /// Checks for a newer version on a thread of its own, of the lowest priority, unless one already works; with
    /// `install`, a newer version's installer is then downloaded and checked, for `main.rs` to run - unless it is
    /// `automatic` and that version's installer already ran without taking (`tried`). `force` (the panel's Reinstall)
    /// takes the channel's version whatever this one is - the same, or even newer - still only as its signed manifest
    /// describes it. `latest`: the newest release published rather than the stable tag (updateChannel).
    pub fn start(&self, install: bool, automatic: bool, force: bool, latest: bool) {
        if self.status().busy() {
            return;
        }
        self.set(Status::Checking);
        let status = self.status.clone();
        let spawned = std::thread::Builder::new().name("update".into()).spawn(move || {
            crate::cpu::lowest_priority();
            let set = |s: Status| {
                if let Ok(mut lock) = status.lock() {
                    *lock = s;
                }
            };
            set(match run(install, automatic, force, latest, &set) {
                Ok(s) => s,
                Err(e) => Status::Failed(e),
            });
        });
        if let Err(e) = spawned {
            self.set(Status::Failed(format!("no thread for the update: {e}")));
        }
    }
}

/// The check, and with `install` the download.
#[cfg(windows)]
fn run(install: bool, automatic: bool, force: bool, latest: bool, set: &dyn Fn(Status)) -> Result<Status, String> {
    let source = Source::new();
    let tag = if latest {
        source.latest()?
    } else {
        String::from_utf8(source.fetch(STABLE_URL, MAX_TAG)?).map_err(|_| "the stable tag is not text")?
    };
    let tag = tag.trim();
    let which = if latest { "latest release's" } else { "stable" };
    let stable = version::parse(tag).ok_or(format!("the {which} tag {tag:?} is not a version"))?;
    let stable = format!("{}.{}.{}", stable.0, stable.1, stable.2);
    if !force && !version::newer(&stable, version::CURRENT) {
        return Ok(Status::UpToDate { stable });
    }
    // Only the tag's own characters go into the URLs: v, digits and dots, which parse() has checked.
    let tag = format!("v{stable}");
    let manifest = source.fetch(&release_url(&tag, signing::MANIFEST), MAX_MANIFEST)?;
    let signature = String::from_utf8(source.fetch(&release_url(&tag, signing::SIGNATURE), MAX_SIGNATURE)?)
        .map_err(|_| "the signature is not text")?;
    let m = signing::verify(&manifest, &signature, signing::PUBLIC_KEYS)?;
    if m.version != stable {
        return Err(format!("the release's manifest is for {}, not {stable}", m.version));
    }
    if m.size > MAX_INSTALLER {
        return Err(format!("the installer would be {} MB", m.size / 1_000_000));
    }
    if !install {
        return Ok(Status::Available { version: stable });
    }
    if automatic && tried().as_deref() == Some(stable.as_str()) {
        return Ok(Status::Stuck { version: stable });
    }
    let path = download(&source, &tag, &m, set)?;
    Ok(Status::Ready { version: stable, path })
}

#[cfg(not(windows))]
fn run(_install: bool, _automatic: bool, _force: bool, _latest: bool, _set: &dyn Fn(Status)) -> Result<Status, String> {
    Err("updates are checked on Windows only".into())
}

fn release_url(tag: &str, file: &str) -> String {
    format!("https://github.com/{REPO}/releases/download/{tag}/{file}")
}

/// Downloads the manifest's installer into %TEMP%\nagare-update, checks its size and SHA-256, and returns its path.
#[cfg(windows)]
fn download(source: &Source, tag: &str, m: &signing::Manifest, set: &dyn Fn(Status)) -> Result<PathBuf, String> {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    let dir = folder();
    // What an earlier download left goes: one installer at a time.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let part = dir.join(format!("{}.part", m.installer));
    let path = dir.join(&m.installer);

    let mut reader = source.open(&release_url(tag, &m.installer), m.size)?;
    let mut file = std::fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?;
    let mut hash = Sha256::new();
    let (mut done, mut buf, mut shown) = (0u64, vec![0u8; 64 * 1024], 0u64);
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("the download broke off: {e}"))?;
        if n == 0 {
            break;
        }
        done += n as u64;
        if done > m.size {
            return Err("the installer is larger than its manifest says".into());
        }
        hash.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| format!("{}: {e}", part.display()))?;
        if done - shown >= 256 * 1024 || done == m.size {
            shown = done;
            set(Status::Downloading { version: m.version.clone(), done, total: m.size });
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    if done != m.size {
        return Err(format!("the installer is {done} bytes, its manifest says {}", m.size));
    }
    if signing::hex(&hash.finalize()) != m.sha256 {
        let _ = std::fs::remove_file(&part);
        return Err("the installer's SHA-256 is not its manifest's".into());
    }
    std::fs::rename(&part, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// The folder updates are downloaded into, and the file in it naming the version whose installer last ran.
fn folder() -> PathBuf {
    std::env::temp_dir().join("nagare-update")
}
const TRIED: &str = "tried.txt";

/// The version whose installer last ran, while Nagare is still older than it: that install did not take.
pub fn tried() -> Option<String> {
    let v = std::fs::read_to_string(folder().join(TRIED)).ok()?;
    let v = v.trim().to_string();
    version_newer(&v).then_some(v)
}

/// Clears away a finished update: once Nagare is the version last installed, or newer, the folder and the installer
/// in it go.
pub fn tidy() {
    let dir = folder();
    if let Ok(v) = std::fs::read_to_string(dir.join(TRIED)) {
        if !version_newer(v.trim()) {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

#[cfg(windows)]
fn version_newer(v: &str) -> bool {
    version::newer(v, version::CURRENT)
}

#[cfg(not(windows))]
fn version_newer(_v: &str) -> bool {
    false
}

/// Runs the downloaded installer of `version` silently, on its own: it stops Nagare (and its watchdog), installs, and
/// starts Nagare again. Its log goes beside it, and `tried` names the version first. The updater's own test
/// variables are not handed on, to the installer or the Nagare it starts.
#[cfg(windows)]
pub fn install(path: &std::path::Path, version: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    std::fs::write(folder().join(TRIED), version).map_err(|e| format!("{}: {e}", folder().display()))?;
    let log = path.with_extension("log");
    std::process::Command::new(path)
        .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"])
        .arg(format!("/LOG={}", log.display()))
        .env_remove("NAGARE_UPDATE_SOURCE")
        .env_remove("NAGARE_UPDATE_NOW")
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not start the installer: {e}"))
}

#[cfg(not(windows))]
pub fn install(_path: &std::path::Path, _version: &str) -> Result<(), String> {
    Err("updates install on Windows only".into())
}

/// Where the files come from: GitHub, or a folder (NAGARE_UPDATE_SOURCE).
enum Source {
    #[cfg(windows)]
    Web(ureq::Agent),
    Folder(PathBuf),
    #[cfg(not(windows))]
    None,
}

impl Source {
    fn new() -> Self {
        if let Some(dir) = std::env::var_os("NAGARE_UPDATE_SOURCE") {
            return Source::Folder(PathBuf::from(dir));
        }
        #[cfg(windows)]
        return Source::Web(agent());
        #[cfg(not(windows))]
        Source::None
    }

    /// The whole of `url`'s body, at most `limit` bytes.
    fn fetch(&self, url: &str, limit: u64) -> Result<Vec<u8>, String> {
        use std::io::Read;
        let mut out = Vec::new();
        self.open(url, limit)?.read_to_end(&mut out).map_err(|e| format!("reading {}: {e}", host(url)))?;
        Ok(out)
    }

    /// The latest release's tag (LATEST_URL), or latest.txt's in a folder.
    fn latest(&self) -> Result<String, String> {
        match self {
            #[cfg(windows)]
            Source::Web(agent) => latest_tag(agent),
            Source::Folder(_) => {
                String::from_utf8(self.fetch("latest.txt", MAX_TAG)?).map_err(|_| "latest.txt is not text".into())
            }
            #[cfg(not(windows))]
            Source::None => Err("updates are checked on Windows only".into()),
        }
    }

    /// `url`'s body as a reader, which fails past `limit` bytes.
    fn open(&self, url: &str, limit: u64) -> Result<Box<dyn std::io::Read + Send>, String> {
        match self {
            #[cfg(windows)]
            Source::Web(agent) => get(agent, url, limit),
            Source::Folder(dir) => {
                let name = url.rsplit('/').next().unwrap_or_default();
                let path = dir.join(name);
                let file = std::fs::File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                use std::io::Read;
                Ok(Box::new(file.take(limit + 1)))
            }
            #[cfg(not(windows))]
            Source::None => Err("updates are checked on Windows only".into()),
        }
    }
}

/// The host `url` names, for messages.
fn host(url: &str) -> &str {
    url.strip_prefix("https://").unwrap_or(url).split(['/', ':', '?']).next().unwrap_or_default()
}

/// Whether a request may go to `url`: HTTPS, to one of HOSTS, on the default port, with no user name in it.
pub fn allowed(url: &str) -> Result<(), String> {
    let Some(rest) = url.strip_prefix("https://") else {
        return Err(format!("refused a request that is not HTTPS: {url}"));
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.contains('@') || authority.contains(':') {
        return Err(format!("refused a request with a user or a port: {url}"));
    }
    if !HOSTS.contains(&authority.to_ascii_lowercase().as_str()) {
        return Err(format!("refused a request to {authority}, which is not GitHub"));
    }
    Ok(())
}

/// The HTTP client: HTTPS only, Mozilla's roots, no redirects followed by itself (`get` follows them, checking each),
/// statuses handed back rather than taken for errors, and timeouts.
#[cfg(windows)]
fn agent() -> ureq::Agent {
    use std::time::Duration;
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::WebPki).build();
    ureq::Agent::config_builder()
        .https_only(true)
        .tls_config(tls)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .http_status_as_error(false)
        .user_agent(format!("Nagare/{} (update check)", version::CURRENT))
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_global(Some(Duration::from_secs(600)))
        .build()
        .into()
}

/// GETs `url`, following at most five redirects, each checked by `allowed`, and returns the body's reader, which fails
/// a byte past `limit` bytes.
#[cfg(windows)]
fn get(agent: &ureq::Agent, url: &str, limit: u64) -> Result<Box<dyn std::io::Read + Send>, String> {
    let mut url = url.to_string();
    for _ in 0..=5 {
        allowed(&url)?;
        let response = agent.get(&url).call().map_err(|e| format!("{}: {e}", host(&url)))?;
        let status = response.status().as_u16();
        if (300..400).contains(&status) {
            let location = response
                .headers()
                .get("location")
                .and_then(|l| l.to_str().ok())
                .ok_or(format!("{} redirected nowhere", host(&url)))?;
            // A redirect within the host may leave the host out.
            url = if location.starts_with('/') { format!("https://{}{location}", host(&url)) } else { location.to_string() };
            continue;
        }
        if status != 200 {
            return Err(format!("{} answered {status}", host(&url)));
        }
        if response.headers().get("content-length").and_then(|l| l.to_str().ok()).and_then(|l| l.parse::<u64>().ok()).is_some_and(|n| n > limit) {
            return Err(format!("{} sent more than expected", host(&url)));
        }
        // ureq's limit takes the read that finds the body's end, after exactly \`limit\` bytes, for one too many: a body
        // of just \`limit\` bytes - an installer of exactly its manifest's size - would fail at its last read. A byte
        // more lets that read through; what came is held to its size by the caller (the manifest's size, exactly).
        return Ok(Box::new(response.into_body().into_with_config().limit(limit.saturating_add(1)).reader()));
    }
    Err("too many redirects".into())
}

/// The latest release's tag, read off the redirect LATEST_URL answers a HEAD request with - the release's page,
/// .../releases/tag/TAG - which is all that is read: no page comes. With no release published, GitHub redirects to the
/// releases' list instead, which has no tag.
#[cfg(windows)]
fn latest_tag(agent: &ureq::Agent) -> Result<String, String> {
    allowed(LATEST_URL)?;
    let response = agent.head(LATEST_URL).call().map_err(|e| format!("{}: {e}", host(LATEST_URL)))?;
    let status = response.status().as_u16();
    if !(300..400).contains(&status) {
        return Err(format!("{} answered {status} for the latest release", host(LATEST_URL)));
    }
    let location = response.headers().get("location").and_then(|l| l.to_str().ok()).ok_or("github.com named no latest release")?;
    latest_from(location)
}

/// The tag in the address GitHub's latest-release link redirects to.
fn latest_from(location: &str) -> Result<String, String> {
    let location = location.strip_prefix("https://github.com").unwrap_or(location);
    let prefix = format!("/{REPO}/releases/tag/");
    location
        .strip_prefix(prefix.as_str())
        .filter(|tag| !tag.is_empty() && !tag.contains(['/', '?', '#']))
        .map(str::to_string)
        .ok_or("no release is published yet".into())
}

/// Fetches `url` the way the updater fetches its files, and says where it ended up and how much came: `--update-fetch`,
/// to try the client against GitHub.
#[cfg(windows)]
pub fn probe(url: &str) -> Result<String, String> {
    use std::io::Read;
    let agent = agent();
    let mut body = Vec::new();
    get(&agent, url, MAX_INSTALLER)?.read_to_end(&mut body).map_err(|e| e.to_string())?;
    Ok(format!("{} bytes from {url}", body.len()))
}

/// A random number of seconds between `low` and `high`.
pub fn random_secs(low: f64, high: f64) -> f64 {
    #[cfg(windows)]
    let bits = {
        let mut b = [0u8; 8];
        let _ = getrandom::fill(&mut b);
        u64::from_le_bytes(b)
    };
    // Elsewhere nothing is checked; the clock's nanoseconds will do.
    #[cfg(not(windows))]
    let bits = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
        .wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let unit = (bits >> 11) as f64 / (1u64 << 53) as f64;
    low + (high - low) * unit
}

/// Seconds since 1970, by the system's clock.
pub fn unix_now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest() {
        assert_eq!(latest_from("https://github.com/bungholesurfers/Nagare/releases/tag/v0.1.3").as_deref(), Ok("v0.1.3"));
        assert_eq!(latest_from("/bungholesurfers/Nagare/releases/tag/v1.2.3").as_deref(), Ok("v1.2.3"));
        assert!(latest_from("https://github.com/bungholesurfers/Nagare/releases").is_err());
        assert!(latest_from("https://github.com/someone/Else/releases/tag/v9.9.9").is_err());
        assert!(latest_from("https://github.com/bungholesurfers/Nagare/releases/tag/v1/x").is_err());
    }

    #[test]
    fn hosts() {
        assert!(allowed("https://github.com/bungholesurfers/Nagare/releases/download/v0.1.1/x").is_ok());
        assert!(allowed("https://release-assets.githubusercontent.com/github-production-release-asset/1?sig=2").is_ok());
        assert!(allowed("http://github.com/x").is_err());
        assert!(allowed("https://github.com.evil.example/x").is_err());
        assert!(allowed("https://evil.example/github.com").is_err());
        assert!(allowed("https://github.com:8443/x").is_err());
        assert!(allowed("https://user@github.com/x").is_err());
        assert!(allowed("HTTPS://github.com/x").is_err());
    }

    #[test]
    fn random() {
        for _ in 0..100 {
            let s = random_secs(60.0, 3600.0);
            assert!((60.0..=3600.0).contains(&s));
        }
    }

    /// The client's TLS refuses bad certificates: expired, for another name, self-signed, from an unknown authority.
    /// It goes to the network, so it runs only when asked: cargo test --release -- --ignored tls
    /// The latest release's tag, read off GitHub's redirect - on the network, so only with --ignored.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn latest_live() {
        let tag = latest_tag(&agent()).expect("no latest release");
        assert!(version::parse(&tag).is_some(), "{tag:?}");
        eprintln!("latest release: {tag}");
    }

    #[cfg(windows)]
    #[test]
    #[ignore]
    fn tls() {
        let agent = agent();
        for host in ["expired", "wrong.host", "self-signed", "untrusted-root"] {
            let url = format!("https://{host}.badssl.com/");
            assert!(agent.get(&url).call().is_err(), "{url} was accepted");
        }
        let good = agent.get("https://github.com/").call().expect("github.com refused");
        assert!(good.status().as_u16() < 500);
    }

    #[cfg(windows)]
    #[test]
    fn manifests() {
        use ed25519_dalek::Signer;
        let key = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let public = signing::hex(&key.verifying_key().to_bytes());
        let m = signing::Manifest { version: "0.1.1".into(), installer: "nagare-setup-0.1.1.exe".into(), size: 3, sha256: "ab".repeat(32) };
        let json = signing::manifest_json(&m);
        let sig = signing::hex(&key.sign(json.as_bytes()).to_bytes());
        assert_eq!(signing::verify(json.as_bytes(), &sig, &[&public]), Ok(m.clone()));
        // Another key, a changed byte, another installer's name: refused.
        assert!(signing::verify(json.as_bytes(), &sig, signing::PUBLIC_KEYS).is_err());
        assert!(signing::verify(json.replace("0.1.1", "0.1.2").as_bytes(), &sig, &[&public]).is_err());
        let odd = signing::Manifest { installer: "evil.exe".into(), ..m };
        let json = signing::manifest_json(&odd);
        let sig = signing::hex(&key.sign(json.as_bytes()).to_bytes());
        assert!(signing::verify(json.as_bytes(), &sig, &[&public]).is_err());
    }
}
