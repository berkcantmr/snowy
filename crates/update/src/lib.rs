//! In-app updates from GitHub Releases.
//!
//! Privacy: the only request made is "what is the latest release of the Snowy repository"
//! (plus the installer download when the user asks for it). Nothing identifying is sent
//! beyond the `User-Agent: Snowy/<version>` header, and the on-disk state file holds only
//! version numbers and a timestamp.
//!
//! Security: downloads are restricted to https GitHub hosts (every redirect hop is checked),
//! capped at 60 MB, and verified against the `.sha256` asset of the same release before the
//! installer is ever started.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const REPO: &str = "berkcantmr/snowy";

/// Hard cap for any downloaded installer.
pub const MAX_DOWNLOAD: u64 = 60 * 1024 * 1024;
const LOCAL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A newer release that can be installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// Version without the leading "v", e.g. "0.2.0".
    pub version: String,
    /// Release notes (markdown as written on GitHub).
    pub notes: String,
    pub setup_url: String,
    pub sha256_url: String,
    /// Size of the installer in bytes, as reported by GitHub.
    pub size: u64,
}

// ---------------------------------------------------------------- versions

fn parse_version(s: &str) -> Option<Vec<u64>> {
    let s = s.trim();
    let s = s.strip_prefix('v').or_else(|| s.strip_prefix('V')).unwrap_or(s);
    if s.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for p in s.split('.') {
        if p.is_empty() || p.len() > 9 || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        parts.push(p.parse().ok()?);
    }
    (parts.len() <= 4).then_some(parts)
}

/// True when `remote` is a strictly higher version than `local` (numeric, per component).
/// Malformed input on either side is never "newer".
pub fn is_newer(remote: &str, local: &str) -> bool {
    let (Some(mut r), Some(mut l)) = (parse_version(remote), parse_version(local)) else {
        return false;
    };
    let n = r.len().max(l.len());
    r.resize(n, 0);
    l.resize(n, 0);
    r > l
}

// ---------------------------------------------------------------- release JSON

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    #[serde(default)]
    size: u64,
    browser_download_url: String,
}

/// Parses a GitHub "latest release" response. `None` when it is a draft/prerelease, not
/// newer than `local`, or lacks the exact setup + checksum assets.
fn parse_release(body: &str, local: &str) -> Result<Option<Release>> {
    let r: ApiRelease = serde_json::from_str(body).context("unexpected release response")?;
    if r.draft || r.prerelease || !is_newer(&r.tag_name, local) {
        return Ok(None);
    }
    let version = r.tag_name.trim().trim_start_matches(['v', 'V']).to_string();
    let setup_name = format!("Snowy-{version}-setup.exe");
    let sha_name = format!("{setup_name}.sha256");
    let find = |name: &str| r.assets.iter().find(|a| a.name == name);
    let (Some(setup), Some(sha)) = (find(&setup_name), find(&sha_name)) else {
        return Ok(None);
    };
    Ok(Some(Release {
        version,
        notes: r.body.unwrap_or_default(),
        setup_url: setup.browser_download_url.clone(),
        sha256_url: sha.browser_download_url.clone(),
        size: setup.size,
    }))
}

// ---------------------------------------------------------------- url policy

/// Only https URLs on GitHub's own hosts (default port, no credentials) are ever requested.
pub fn is_allowed_url(url: &str) -> bool {
    const HOSTS: [&str; 3] = ["github.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com"];
    let Some(rest) = url.get(..8).filter(|p| p.eq_ignore_ascii_case("https://")).map(|_| &url[8..]) else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains(['@', '\\', ' ', '%']) {
        return false;
    }
    let host = match authority.rsplit_once(':') {
        Some((h, "443")) => h,
        Some(_) => return false,
        None => authority,
    };
    let host = host.to_ascii_lowercase();
    HOSTS.contains(&host.as_str())
}

// ---------------------------------------------------------------- checksum + streaming

/// First whitespace-separated token of a `.sha256` file, if it is 64 hex digits (lowercased).
pub fn parse_sha256(text: &str) -> Option<String> {
    let tok = text.trim_start_matches('\u{feff}').split_whitespace().next()?;
    (tok.len() == 64 && tok.bytes().all(|b| b.is_ascii_hexdigit())).then(|| tok.to_ascii_lowercase())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Copies `src` to `dst`, hashing on the way. Fails once more than `cap` bytes arrive.
/// `progress(done, total)` is called per chunk (`total` is the expected size or 0).
/// Returns (bytes, lowercase sha256 hex).
fn copy_hashed(
    mut src: impl Read,
    mut dst: impl Write,
    cap: u64,
    total: u64,
    mut progress: impl FnMut(u64, u64),
) -> Result<(u64, String)> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    loop {
        let n = src.read(&mut buf).context("download interrupted")?;
        if n == 0 {
            break;
        }
        done += n as u64;
        if done > cap {
            bail!("download is larger than the {} MB limit", cap / 1024 / 1024);
        }
        hasher.update(&buf[..n]);
        dst.write_all(&buf[..n]).context("could not write the installer")?;
        progress(done, total);
    }
    dst.flush()?;
    Ok((done, hex(&hasher.finalize())))
}

// ---------------------------------------------------------------- network

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(10))
        .timeout_write(Duration::from_secs(10))
        // Redirects are followed by hand so every hop can be checked against the allow-list.
        .redirects(0)
        .user_agent(concat!("Snowy/", env!("CARGO_PKG_VERSION")))
        .build()
}

fn get_checked(agent: &ureq::Agent, url: &str) -> Result<ureq::Response> {
    let mut url = url.to_string();
    for _ in 0..6 {
        if !is_allowed_url(&url) {
            bail!("refusing to download from an untrusted address");
        }
        let resp = match agent.get(&url).set("Accept", "application/octet-stream").call() {
            Ok(r) => r,
            Err(ureq::Error::Status(code, _)) => bail!("server answered HTTP {code}"),
            Err(e) => bail!("network error: {e}"),
        };
        if (300..400).contains(&resp.status()) {
            url = resp.header("Location").ok_or_else(|| anyhow!("redirect without a target"))?.to_string();
            continue;
        }
        return Ok(resp);
    }
    bail!("too many redirects")
}

/// Asks GitHub for the latest release. `Ok(None)` when there is nothing newer to install
/// (also when the repository has no releases yet).
pub fn check_latest() -> Result<Option<Release>> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let resp = match agent().get(&url).set("Accept", "application/vnd.github+json").call() {
        Ok(r) => r,
        Err(ureq::Error::Status(404, _)) => {
            record_check_in(&state_dir(), None);
            return Ok(None);
        }
        Err(ureq::Error::Status(403 | 429, _)) => bail!("GitHub is rate-limiting requests; try again later"),
        Err(ureq::Error::Status(code, _)) => bail!("GitHub answered HTTP {code}"),
        Err(e) => bail!("could not reach GitHub: {e}"),
    };
    let mut body = String::new();
    resp.into_reader().take(2 * 1024 * 1024).read_to_string(&mut body).context("could not read the response")?;
    let release = parse_release(&body, LOCAL_VERSION)?;
    record_check_in(&state_dir(), release.as_ref().map(|r| r.version.as_str()));
    Ok(release)
}

/// Downloads the installer, verifies its SHA-256 against the release's checksum asset and
/// returns the verified file. On a mismatch the file is deleted and an error returned.
pub fn download_verified(r: &Release, progress: impl FnMut(u64, u64)) -> Result<PathBuf> {
    if parse_version(&r.version).is_none() {
        bail!("invalid release version");
    }
    let dir = std::env::temp_dir().join("snowy-update");
    download_into(r, &dir, progress)
}

fn download_into(r: &Release, dir: &Path, mut progress: impl FnMut(u64, u64)) -> Result<PathBuf> {
    if r.size > MAX_DOWNLOAD {
        bail!("installer is larger than the {} MB limit", MAX_DOWNLOAD / 1024 / 1024);
    }
    let agent = agent();

    let mut sha_text = String::new();
    get_checked(&agent, &r.sha256_url)?
        .into_reader()
        .take(4096)
        .read_to_string(&mut sha_text)
        .context("could not read the checksum")?;
    let expected = parse_sha256(&sha_text).ok_or_else(|| anyhow!("the checksum file is malformed"))?;

    let resp = get_checked(&agent, &r.setup_url)?;
    let total = resp.header("Content-Length").and_then(|v| v.parse::<u64>().ok()).unwrap_or(r.size);
    if total > MAX_DOWNLOAD {
        bail!("installer is larger than the {} MB limit", MAX_DOWNLOAD / 1024 / 1024);
    }

    std::fs::create_dir_all(dir)?;
    let name = format!("Snowy-{}-setup.exe", r.version);
    let part = dir.join(format!("{name}.part"));
    let fin = dir.join(&name);
    let _ = std::fs::remove_file(&fin);
    let result = (|| -> Result<()> {
        let file = std::fs::File::create(&part).context("could not create the temporary file")?;
        let (_, actual) = copy_hashed(resp.into_reader(), std::io::BufWriter::new(file), MAX_DOWNLOAD, total, &mut progress)?;
        if actual != expected {
            bail!("checksum mismatch: the download is corrupt or has been tampered with; it was deleted");
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    std::fs::rename(&part, &fin).context("could not finish the download")?;
    Ok(fin)
}

/// Starts the installer silently (it closes the running Snowy and restarts it afterwards).
/// The caller should exit right after.
pub fn launch_installer(path: &Path) -> Result<()> {
    std::process::Command::new(path)
        .args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CLOSEAPPLICATIONS", "/UPDATE=1"])
        .spawn()
        .context("could not start the installer")?;
    Ok(())
}

// ---------------------------------------------------------------- state file

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct State {
    /// Unix seconds of the last successful check.
    last_check: u64,
    /// Latest release version seen that was newer than the installed one ("" = none).
    latest_version: String,
    skipped_version: String,
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn state_dir() -> PathBuf {
    snowy_core::config::Settings::dir()
}

fn read_state_in(dir: &Path) -> State {
    std::fs::read_to_string(dir.join("update.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn write_state_in(dir: &Path, s: &State) {
    let go = || -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join("update.json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(s).map_err(std::io::Error::other)?)?;
        std::fs::rename(tmp, dir.join("update.json"))
    };
    if let Err(e) = go() {
        log::warn!("update state not written: {e}");
    }
}

fn record_check_in(dir: &Path, latest: Option<&str>) {
    let mut s = read_state_in(dir);
    s.last_check = now_unix();
    s.latest_version = latest.unwrap_or("").to_string();
    write_state_in(dir, &s);
}

/// Marks `version` as skipped: no more tray prompts for it.
pub fn skip(version: &str) {
    let dir = state_dir();
    let mut s = read_state_in(&dir);
    s.skipped_version = version.to_string();
    write_state_in(&dir, &s);
}

/// Latest known newer version from the last successful check (any process), if any.
pub fn cached_latest() -> Option<String> {
    let s = read_state_in(&state_dir());
    (!s.latest_version.is_empty() && is_newer(&s.latest_version, LOCAL_VERSION)).then_some(s.latest_version)
}

// ---------------------------------------------------------------- watcher

const FIRST_DELAY: Duration = Duration::from_secs(20);
const INTERVAL: Duration = Duration::from_secs(24 * 3600);
const RETRY: Duration = Duration::from_secs(3600);

/// Background checker: first check 20 s after start, then daily; stops on drop.
pub struct UpdateWatch {
    state: Arc<Mutex<Option<Release>>>,
    stop: Sender<()>,
}

impl UpdateWatch {
    pub fn start() -> Self {
        let state = Arc::new(Mutex::new(None));
        let (stop, rx) = mpsc::channel::<()>();
        let shared = Arc::clone(&state);
        let spawned = std::thread::Builder::new().name("snowy-update".into()).spawn(move || {
            let mut wait = FIRST_DELAY;
            loop {
                match rx.recv_timeout(wait) {
                    Err(RecvTimeoutError::Timeout) => {}
                    _ => return,
                }
                match check_latest() {
                    Ok(found) => {
                        wait = INTERVAL;
                        // Dropped while the request was in flight: discard the result.
                        if !matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)) {
                            return;
                        }
                        *shared.lock().unwrap_or_else(|e| e.into_inner()) = found;
                    }
                    Err(e) => {
                        wait = RETRY;
                        log::warn!("update check failed: {e:#}");
                    }
                }
            }
        });
        if let Err(e) = spawned {
            log::warn!("update thread not started: {e}");
        }
        Self { state, stop }
    }

    /// A newer release that the user has not skipped, if one was found.
    pub fn current(&self) -> Option<Release> {
        let r = self.state.lock().unwrap_or_else(|e| e.into_inner()).clone()?;
        (read_state_in(&state_dir()).skipped_version != r.version).then_some(r)
    }
}

impl Drop for UpdateWatch {
    fn drop(&mut self) {
        let _ = self.stop.send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn newer_versions() {
        assert!(is_newer("0.1.10", "0.1.9"));
        assert!(is_newer("v0.2.0", "0.1.9"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(is_newer("0.1.0.1", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0.0", "0.1.0"));
        assert!(!is_newer("0.1.9", "0.1.10"));
        for bad in ["", "v", "abc", "1..2", "1.2.x", "1.2.3-beta", "-1.0", "1.2.3.4.5", "99999999999.0"] {
            assert!(!is_newer(bad, "0.0.1"), "{bad}");
            assert!(!is_newer("9.9.9", bad), "{bad}");
        }
    }

    #[test]
    fn sha_parsing() {
        let h = "A".repeat(32) + &"b1".repeat(16);
        assert_eq!(parse_sha256(&format!("{h}  Snowy-0.2.0-setup.exe\n")), Some(h.to_ascii_lowercase()));
        assert_eq!(parse_sha256(&h.to_ascii_lowercase()), Some(h.to_ascii_lowercase()));
        assert_eq!(parse_sha256(&format!("\u{feff}{h}\r\n")), Some(h.to_ascii_lowercase()));
        assert_eq!(parse_sha256(""), None);
        assert_eq!(parse_sha256("abc123  file"), None);
        assert_eq!(parse_sha256(&format!("{}  f", "g".repeat(64))), None);
        assert_eq!(parse_sha256(&format!("{h}0  f")), None);
    }

    #[test]
    fn host_allow_list() {
        for ok in [
            "https://github.com/berkcantmr/snowy/releases/download/v0.2.0/Snowy-0.2.0-setup.exe",
            "https://objects.githubusercontent.com/github-production-release-asset/1?x=y",
            "https://release-assets.githubusercontent.com/a/b",
            "HTTPS://GitHub.com/x",
            "https://github.com:443/x",
            "https://github.com",
        ] {
            assert!(is_allowed_url(ok), "{ok}");
        }
        for bad in [
            "http://github.com/x",
            "https://evil.com/x",
            "https://github.com@evil.com/",
            "https://evil.com@github.com/",
            "https://github.com:8443/x",
            "https://github.com.evil.com/x",
            "https://evilgithub.com/x",
            "https://api.github.com/x",
            "https://github.com\\@evil.com/",
            "https://github.com%2f@evil.com/",
            "https://githubusercontent.com/x",
            "ftp://github.com/x",
            "//github.com/x",
            "https://",
            "",
        ] {
            assert!(!is_allowed_url(bad), "{bad}");
        }
    }

    fn sample(tag: &str, assets: &str) -> String {
        format!(
            r#"{{"url":"x","tag_name":"{tag}","name":"Snowy","draft":false,"prerelease":false,
            "body":"- New things","assets":[{assets}]}}"#
        )
    }

    const GOOD_ASSETS: &str = r#"
      {"name":"Snowy-0.2.0-portable.zip","size":5,"browser_download_url":"https://github.com/o/r/releases/download/v0.2.0/Snowy-0.2.0-portable.zip"},
      {"name":"Snowy-0.2.0-setup.exe","size":1234567,"browser_download_url":"https://github.com/o/r/releases/download/v0.2.0/Snowy-0.2.0-setup.exe"},
      {"name":"Snowy-0.2.0-setup.exe.sha256","size":90,"browser_download_url":"https://github.com/o/r/releases/download/v0.2.0/Snowy-0.2.0-setup.exe.sha256"}"#;

    #[test]
    fn release_parsing() {
        let r = parse_release(&sample("v0.2.0", GOOD_ASSETS), "0.1.0").unwrap().unwrap();
        assert_eq!(r.version, "0.2.0");
        assert_eq!(r.size, 1234567);
        assert_eq!(r.notes, "- New things");
        assert!(r.setup_url.ends_with("Snowy-0.2.0-setup.exe"));
        assert!(r.sha256_url.ends_with("Snowy-0.2.0-setup.exe.sha256"));
        // Not newer.
        assert_eq!(parse_release(&sample("v0.2.0", GOOD_ASSETS), "0.2.0").unwrap(), None);
        // Missing checksum / missing setup / assets named for another version.
        let no_sha = r#"{"name":"Snowy-0.2.0-setup.exe","size":1,"browser_download_url":"https://github.com/a"}"#;
        assert_eq!(parse_release(&sample("v0.2.0", no_sha), "0.1.0").unwrap(), None);
        let sha_only = r#"{"name":"Snowy-0.2.0-setup.exe.sha256","size":1,"browser_download_url":"https://github.com/a"}"#;
        assert_eq!(parse_release(&sample("v0.2.0", sha_only), "0.1.0").unwrap(), None);
        assert_eq!(parse_release(&sample("v0.3.0", GOOD_ASSETS), "0.1.0").unwrap(), None);
        assert_eq!(parse_release(&sample("v0.2.0", ""), "0.1.0").unwrap(), None);
        // Prerelease / draft are ignored; malformed JSON is an error.
        let pre = sample("v0.2.0", GOOD_ASSETS).replace(r#""prerelease":false"#, r#""prerelease":true"#);
        assert_eq!(parse_release(&pre, "0.1.0").unwrap(), None);
        let draft = sample("v0.2.0", GOOD_ASSETS).replace(r#""draft":false"#, r#""draft":true"#);
        assert_eq!(parse_release(&draft, "0.1.0").unwrap(), None);
        assert!(parse_release("not json", "0.1.0").is_err());
        assert!(parse_release(r#"{"message":"Not Found"}"#, "0.1.0").is_err());
    }

    #[test]
    fn hashing_and_cap() {
        let data = vec![7u8; 200_000];
        let mut out = Vec::new();
        let mut last = (0, 0);
        let (n, h) = copy_hashed(Cursor::new(&data), &mut out, 1_000_000, 200_000, |d, t| last = (d, t)).unwrap();
        assert_eq!((n, last), (200_000, (200_000, 200_000)));
        assert_eq!(out, data);
        assert_eq!(h, hex(&Sha256::digest(&data)));
        // Known vector: sha256("abc").
        let (_, h) = copy_hashed(Cursor::new(b"abc"), std::io::sink(), 10, 3, |_, _| {}).unwrap();
        assert_eq!(h, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        // Exactly at the cap is fine; one byte over fails.
        assert!(copy_hashed(Cursor::new(vec![0u8; 100]), std::io::sink(), 100, 0, |_, _| {}).is_ok());
        assert!(copy_hashed(Cursor::new(vec![0u8; 101]), std::io::sink(), 100, 0, |_, _| {}).is_err());
    }

    #[test]
    fn oversized_release_rejected_before_any_request() {
        let r = Release {
            version: "9.9.9".into(),
            notes: String::new(),
            setup_url: "https://github.com/x".into(),
            sha256_url: "https://github.com/y".into(),
            size: MAX_DOWNLOAD + 1,
        };
        assert!(download_verified(&r, |_, _| {}).unwrap_err().to_string().contains("limit"));
        let bad = Release { version: "../evil".into(), size: 1, ..r };
        assert!(download_verified(&bad, |_, _| {}).is_err());
    }

    #[test]
    fn state_roundtrip_and_privacy() {
        let dir = std::env::temp_dir().join(format!("snowy-update-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(read_state_in(&dir), State::default());
        write_state_in(&dir, &State { skipped_version: "0.3.0".into(), ..State::default() });
        record_check_in(&dir, Some("0.4.0"));
        let s = read_state_in(&dir);
        assert_eq!((s.latest_version.as_str(), s.skipped_version.as_str()), ("0.4.0", "0.3.0"));
        assert!(s.last_check > 0);
        record_check_in(&dir, None);
        assert_eq!(read_state_in(&dir).latest_version, "");
        let text = std::fs::read_to_string(dir.join("update.json")).unwrap();
        assert!(text.contains("last_check") && text.contains("skipped_version"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn drop_is_prompt() {
        let t = std::time::Instant::now();
        drop(UpdateWatch::start());
        assert!(t.elapsed() < Duration::from_millis(200));
    }

    #[test]
    #[ignore]
    fn live() {
        let r = check_latest().expect("live check failed");
        println!("live check_latest: {r:?}");
    }
}
