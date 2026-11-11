//! Checking for a newer Odeion and installing it: the latest release on GitHub, asked for
//! only when Check for Updates is chosen in the menu bar, and downloaded only when its
//! Install is. These are the only network requests Odeion makes.
//!
//! An update is installed as Zed installs its own: the download is unpacked beside the
//! installed copy and swapped in for it while Odeion runs on, so the next start is the
//! new version. Nothing is installed whose signature doesn't check out against
//! `PUBLIC_KEY`: the Release workflow signs each zip with the secret half.

use minisign_verify::{PublicKey, Signature};
use serde::Deserialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const LATEST: &str = "https://api.github.com/repos/timvancann/odeion/releases/latest";

/// The public half of the minisign key the Release workflow signs each zip with (the
/// secret half is the repo's `MINISIGN_KEY` secret, `docs/releasing.md`).
const PUBLIC_KEY: &str = "RWTQS/xHn8DfL2+OjjlHbGM5BkM5xG8wpdDxrv0GB6Q2Ahk51b32j3mZ";

/// The most a download may be: the Mac zip was 18 MB at 0.4.0.
const MOST: u64 = 300 << 20;

/// What the zip holds, whatever the installed copy is called.
const UNPACKED: &str = if cfg!(target_os = "macos") { "Odeion.app" } else { "odeion.exe" };

/// The folder beside the installed copy an update is unpacked in, which the old copy
/// is moved into; removed after, or when Odeion next starts (`tidy`).
const STAGING: &str = ".odeion-update";

/// A published release: its version and where to get it.
#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    /// This system's download (`DOWNLOAD`), else the release's page.
    pub download: String,
    /// The download's signature (`<DOWNLOAD>.minisig`), when it has one: without it the
    /// release can only be downloaded by hand.
    pub signature: Option<String>,
}

/// The download each release carries for this system, under a name that never changes.
const DOWNLOAD: Option<&str> = if cfg!(target_os = "macos") {
    Some("Odeion-macOS.zip")
} else if cfg!(windows) {
    Some("Odeion-Windows.zip")
} else {
    None
};

/// What a check found.
#[derive(Debug, Clone, PartialEq)]
pub enum Found {
    /// Up to date: the latest release's version, which may be older than a build from
    /// source.
    Latest(String),
    Newer(Release),
}

#[derive(Deserialize)]
struct Answer {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// GitHub's answer for the latest release, as a `Release`.
pub fn parse(body: &str) -> Result<Release, String> {
    let answer: Answer = serde_json::from_str(body).map_err(|_| "GitHub's answer didn't make sense".to_string())?;
    let asset = |name: String| answer.assets.iter().find(|a| a.name == name).map(|a| a.browser_download_url.clone());
    let zip = DOWNLOAD.and_then(|name| asset(name.to_string()));
    let signature = DOWNLOAD.filter(|_| zip.is_some()).and_then(|name| asset(format!("{name}.minisig")));
    Ok(Release {
        version: answer.tag_name.trim_start_matches('v').to_string(),
        download: zip.unwrap_or(answer.html_url),
        signature,
    })
}

/// Whether version `latest` comes after `current`: dotted numbers, a leading `v` allowed.
/// Only the numbers count: a `-beta.2` or `+build` after them is never newer by itself.
pub fn newer(current: &str, latest: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        let numbers = v.trim_start_matches('v').split(['-', '+']).next().unwrap_or("");
        numbers.split('.').map(|p| p.parse().unwrap_or(0)).collect()
    };
    let (a, b) = (parts(current), parts(latest));
    let n = a.len().max(b.len());
    let at = |v: &[u64], i: usize| v.get(i).copied().unwrap_or(0);
    (0..n).map(|i| (at(&b, i), at(&a, i))).find(|(x, y)| x != y).is_some_and(|(x, y)| x > y)
}

/// The latest release against the version running.
pub fn found(current: &str, latest: Release) -> Found {
    if newer(current, &latest.version) { Found::Newer(latest) } else { Found::Latest(latest.version) }
}

/// Why a check failed, in words for the dialog.
fn why(e: &ureq::Error) -> String {
    use ureq::Error;
    match e {
        Error::StatusCode(404) => "No release is published yet.".into(),
        Error::StatusCode(403 | 429) => "GitHub limits how often it's asked. Try again in a while.".into(),
        Error::StatusCode(500..=599) => "GitHub is having trouble right now. Try again later.".into(),
        Error::StatusCode(code) => format!("GitHub answered with an error ({code})."),
        Error::Timeout(_) => "GitHub didn't answer within 10 seconds. Try again later.".into(),
        Error::HostNotFound | Error::ConnectionFailed | Error::Io(_) => {
            "GitHub couldn't be reached. Are you online?".into()
        }
        _ => "GitHub couldn't be reached.".into(),
    }
}

const USER_AGENT: &str = concat!("Odeion/", env!("CARGO_PKG_VERSION"));

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(timeout)).build().into()
}

/// Asks GitHub, on the calling thread, giving up after ten seconds.
pub fn check() -> Result<Found, String> {
    let mut response = agent(Duration::from_secs(10))
        .get(LATEST)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| why(&e))?;
    let body = response.body_mut().read_to_string().map_err(|_| "GitHub's answer broke off".to_string())?;
    Ok(found(env!("CARGO_PKG_VERSION"), parse(&body)?))
}

/// Where this copy of Odeion is installed, for an update to replace: its app bundle on
/// macOS, `odeion.exe` on Windows. When it can't update itself, why, in words.
pub fn installed() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("Odeion can't tell where it is installed ({e})."))?;
    installed_at(&exe)
}

fn installed_at(exe: &Path) -> Result<PathBuf, String> {
    if cfg!(target_os = "macos") {
        let Some(app) = exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")) else {
            return Err("This Odeion runs from source: pull and build it to update.".into());
        };
        // Opened where it was downloaded, macOS runs a read-only copy from elsewhere.
        if app.components().any(|c| c.as_os_str() == "AppTranslocation") {
            return Err("macOS runs this copy from a read-only place, because it was opened where it \
                was unzipped. Move Odeion to Applications, open it from there and check again."
                .into());
        }
        Ok(app.to_path_buf())
    } else if cfg!(windows) {
        Ok(exe.to_path_buf())
    } else {
        Err("Odeion can't update itself on this system.".into())
    }
}

/// Downloads `release`, checks its signature and puts it in place of the copy at `at`,
/// on the calling thread. The running Odeion carries on as it was; the next start is the
/// new version. Gives the version installed.
pub fn install(release: &Release, at: &Path) -> Result<String, String> {
    let (Some(name), Some(signature)) = (DOWNLOAD, &release.signature) else {
        return Err("This release has no signed download for this system.".into());
    };
    let agent = agent(Duration::from_secs(600));
    let zip = fetch(&agent, &release.download, MOST)?;
    let signature = String::from_utf8(fetch(&agent, signature, 4096)?).unwrap_or_default();
    let version = verify(PUBLIC_KEY, &zip, &signature, name, env!("CARGO_PKG_VERSION"))?;
    put_in_place(&zip, at)?;
    Ok(version)
}

fn fetch(agent: &ureq::Agent, url: &str, most: u64) -> Result<Vec<u8>, String> {
    let mut response = agent.get(url).header("User-Agent", USER_AGENT).call().map_err(|e| why(&e))?;
    response
        .body_mut()
        .with_config()
        .limit(most)
        .read_to_vec()
        .map_err(|e| match e {
            ureq::Error::BodyExceedsLimit(_) => {
                format!("The download is larger than Odeion expects ({} MB), so it was stopped.", most >> 20)
            }
            ureq::Error::Timeout(_) => "The download took too long. Try again.".into(),
            ureq::Error::Io(_) => "The download broke off. Try again.".into(),
            e => why(&e),
        })
}

/// Checks `zip` against `signature` (a `.minisig` file's text) and the public `key`: it
/// must be signed by the key, for the file `name`, and for a version after `current`, all
/// read from the signature's trusted comment ("Odeion-macOS.zip 0.5.0"), which is signed
/// too. So an older release, or the other system's, can't be passed off as the update.
/// Gives the version.
pub fn verify(key: &str, zip: &[u8], signature: &str, name: &str, current: &str) -> Result<String, String> {
    let unsigned = || "The download isn't signed with Odeion's release key, so it wasn't installed.".to_string();
    let key = PublicKey::from_base64(key).map_err(|_| unsigned())?;
    let signature = Signature::decode(signature).map_err(|_| unsigned())?;
    key.verify(zip, &signature, false).map_err(|_| unsigned())?;
    let (signed, version) = signature.trusted_comment().split_once(' ').ok_or_else(unsigned)?;
    if signed != name {
        return Err(format!("The download is signed as {signed}, not {name}, so it wasn't installed."));
    }
    if !newer(current, version) {
        return Err(format!("The download is Odeion {version}, which isn't newer than {current}."));
    }
    Ok(version.to_string())
}

/// Unpacks `zip` beside `at` and swaps what it holds in for `at`, moving the old copy
/// aside: one rename each, so a failure puts the old copy back. Should even that fail,
/// the old copy stays in the staging folder and the error says where.
fn put_in_place(zip: &[u8], at: &Path) -> Result<(), String> {
    let staging = at.with_file_name(STAGING);
    let folder = staging.parent().unwrap_or(Path::new("")).display().to_string();
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir(&staging).map_err(|e| format!("Odeion can't write to {folder}: {e}"))?;
    let old = staging.join("old");
    let mut stranded = false;
    let swapped = (|| {
        let file = staging.join("update.zip");
        fs::write(&file, zip)?;
        unpack(&file, &staging)?;
        let new = staging.join(UNPACKED);
        if !new.exists() {
            return Err(io::Error::other(format!("the download holds no {UNPACKED}")));
        }
        fs::rename(at, &old)?;
        fs::rename(&new, at).inspect_err(|_| stranded = fs::rename(&old, at).is_err())
    })();
    if stranded {
        let e = swapped.err().map(|e| e.to_string()).unwrap_or_default();
        return Err(format!(
            "Odeion couldn't put the update in place ({e}), nor the old copy back: it is at {}. Move it back to {}.",
            old.display(),
            at.display()
        ));
    }
    // A running odeion.exe can't be removed: `tidy` does it next time.
    let _ = fs::remove_dir_all(&staging);
    swapped.map_err(|e| format!("Odeion couldn't put the update in place: {e}"))
}

/// Unpacks with the system's own tool: `ditto` keeps the app's signature and links whole,
/// and Windows' own `tar` reads zips (another `tar` first on the path may not).
fn unpack(zip: &Path, into: &Path) -> io::Result<()> {
    let mut command = if cfg!(windows) {
        let mut tar = windows_tar();
        tar.arg("-xf").arg(zip).arg("-C").arg(into);
        tar
    } else {
        let mut ditto = Command::new("ditto");
        ditto.args(["-x", "-k"]).arg(zip).arg(into);
        ditto
    };
    let status = command.status()?;
    if status.success() { Ok(()) } else { Err(io::Error::other(format!("unpacking failed ({status})"))) }
}

fn windows_tar() -> Command {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    Command::new(Path::new(&root).join(r"System32\tar.exe"))
}

/// Clears what an update left beside this copy: on Windows the old `odeion.exe`, which
/// can't be removed while it runs. Called when Odeion starts.
pub fn tidy() {
    if let Ok(at) = installed() {
        let _ = fs::remove_dir_all(at.with_file_name(STAGING));
    }
}

/// Opens the copy at `at` again as a new process: an app bundle through `open`, so it
/// comes back as the app, a program directly.
pub fn reopen(at: &Path) -> io::Result<()> {
    if at.extension().is_some_and(|e| e == "app") {
        Command::new("open").arg("-n").arg(at).spawn()?;
    } else {
        Command::new(at).spawn()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_as_numbers() {
        assert!(newer("0.2.0", "0.3.0"));
        assert!(newer("0.2.0", "v0.10.0"));
        assert!(newer("0.9.9", "1.0"));
        assert!(!newer("0.2.0", "0.2.0"));
        assert!(!newer("0.2.0", "v0.2"));
        assert!(!newer("0.3.0", "0.2.9"));
        assert!(!newer("0.3.0", "0.3.0-beta.2"));
        assert!(newer("0.3.0", "v0.4.0-rc1"));
    }

    #[test]
    fn a_release_offers_the_mac_zip_or_its_page() {
        let body = r#"{"tag_name": "v0.3.0", "html_url": "https://github.com/timvancann/odeion/releases/tag/v0.3.0",
            "assets": [{"name": "Odeion-Windows.zip", "browser_download_url": "https://x/Odeion-Windows.zip"},
                       {"name": "Odeion-0.3.0.zip", "browser_download_url": "https://x/Odeion-0.3.0.zip"},
                       {"name": "Odeion-macOS.zip", "browser_download_url": "https://x/Odeion-macOS.zip"}]}"#;
        let release = parse(body).unwrap();
        assert_eq!(release.version, "0.3.0");
        // Unsigned, it can only be downloaded by hand.
        assert_eq!(release.signature, None);
        // Each system gets its own download, never the other's.
        let expected = if cfg!(target_os = "macos") {
            "https://x/Odeion-macOS.zip"
        } else if cfg!(windows) {
            "https://x/Odeion-Windows.zip"
        } else {
            "https://github.com/timvancann/odeion/releases/tag/v0.3.0"
        };
        assert_eq!(release.download, expected);
        assert_eq!(found("0.2.0", release.clone()), Found::Newer(release.clone()));
        assert_eq!(found("0.3.0", release.clone()), Found::Latest("0.3.0".into()));
        assert_eq!(found("0.4.0", release), Found::Latest("0.3.0".into()));
        // Without a zip, the page.
        let bare = r#"{"tag_name": "v0.3.0", "html_url": "https://page"}"#;
        assert_eq!(parse(bare).unwrap().download, "https://page");
        assert!(parse("<html>rate limited</html>").is_err());
    }

    #[test]
    fn a_signed_release_carries_its_systems_signature() {
        let body = r#"{"tag_name": "v0.5.0", "html_url": "https://page",
            "assets": [{"name": "Odeion-macOS.zip", "browser_download_url": "https://x/Odeion-macOS.zip"},
                       {"name": "Odeion-macOS.zip.minisig", "browser_download_url": "https://x/Odeion-macOS.zip.minisig"},
                       {"name": "Odeion-Windows.zip", "browser_download_url": "https://x/Odeion-Windows.zip"},
                       {"name": "Odeion-Windows.zip.minisig", "browser_download_url": "https://x/Odeion-Windows.zip.minisig"}]}"#;
        let release = parse(body).unwrap();
        let expected = DOWNLOAD.map(|name| format!("https://x/{name}.minisig"));
        assert_eq!(release.signature, expected);
        // A signature without its zip is no use.
        let lone = r#"{"tag_name": "v0.5.0", "html_url": "https://page",
            "assets": [{"name": "Odeion-macOS.zip.minisig", "browser_download_url": "https://x/s"},
                       {"name": "Odeion-Windows.zip.minisig", "browser_download_url": "https://x/s"}]}"#;
        assert_eq!(parse(lone).unwrap().signature, None);
    }

    /// A key made for these tests (its secret half thrown away), and what it signed:
    /// the bytes `PAYLOAD` as Odeion-macOS.zip 9.0.0, as 0.1.0, and as Odeion-Windows.zip.
    const TEST_KEY: &str = "RWQ/n9tW03A3wdBLWzOwgu/ZMnjc6/YhiBdnYog2OHlw8bEgKcHziaD/";
    const PAYLOAD: &[u8] = b"odeion update payload";
    const SIGNED: &str = "untrusted comment: signature from minisign secret key
RUQ/n9tW03A3wSaSFCCN+XfvyAyX//B8IbnpmoitGrSHQU8l5XT29/egJtjC2rAX/hwEnDnIZoqKs6nLdiK+FgwrKvtrS+ZozQk=
trusted comment: Odeion-macOS.zip 9.0.0
TlUIdFSPi3GYeiSpqY4F+kg2WCS66sz+SXUXJ5YzyLhiP0QHi6hC3z332mPRt5PFcK7fwdNgva5E6Ga1PFBFDQ==
";
    const SIGNED_OLD: &str = "untrusted comment: signature from minisign secret key
RUQ/n9tW03A3wSaSFCCN+XfvyAyX//B8IbnpmoitGrSHQU8l5XT29/egJtjC2rAX/hwEnDnIZoqKs6nLdiK+FgwrKvtrS+ZozQk=
trusted comment: Odeion-macOS.zip 0.1.0
UHhXk9sdqax1pKTd9cry8uoUp7oRoXL2mlaC7rQGrGnTS3Mi9R3KCFPjXUXUhrGFJBq88TEoNVySIyIZe8zJBA==
";
    const SIGNED_WINDOWS: &str = "untrusted comment: signature from minisign secret key
RUQ/n9tW03A3wSaSFCCN+XfvyAyX//B8IbnpmoitGrSHQU8l5XT29/egJtjC2rAX/hwEnDnIZoqKs6nLdiK+FgwrKvtrS+ZozQk=
trusted comment: Odeion-Windows.zip 9.0.0
jaKIC9J4QQlCEJ4OoJnrKcArB0Y+dCVJtOfws+0msdU3LqDQ7wtekozFfMGqIYa0NFZ1mXy4p8fSIvo8ccJlDA==
";

    #[test]
    fn only_a_signed_newer_download_for_this_file_passes() {
        let mac = "Odeion-macOS.zip";
        assert_eq!(verify(TEST_KEY, PAYLOAD, SIGNED, mac, "0.4.0"), Ok("9.0.0".into()));
        // Changed bytes, another key, or no signature at all.
        let refused = |r: Result<String, String>| r.unwrap_err();
        assert!(refused(verify(TEST_KEY, b"odeion update payloaD", SIGNED, mac, "0.4.0")).contains("isn't signed"));
        assert!(refused(verify(PUBLIC_KEY, PAYLOAD, SIGNED, mac, "0.4.0")).contains("isn't signed"));
        assert!(refused(verify(TEST_KEY, PAYLOAD, "", mac, "0.4.0")).contains("isn't signed"));
        assert!(refused(verify(TEST_KEY, PAYLOAD, "<html>not found</html>", mac, "0.4.0")).contains("isn't signed"));
        // A trusted comment edited by hand breaks the signature.
        let edited = SIGNED.replace("9.0.0", "9.9.9");
        assert!(refused(verify(TEST_KEY, PAYLOAD, &edited, mac, "0.4.0")).contains("isn't signed"));
        // Genuine, but an older release or the other system's zip.
        assert!(refused(verify(TEST_KEY, PAYLOAD, SIGNED_OLD, mac, "0.4.0")).contains("isn't newer"));
        assert!(refused(verify(TEST_KEY, PAYLOAD, SIGNED, mac, "9.0.0")).contains("isn't newer"));
        assert!(refused(verify(TEST_KEY, PAYLOAD, SIGNED_WINDOWS, mac, "0.4.0")).contains("signed as Odeion-Windows.zip"));
    }

    #[test]
    fn the_release_key_reads() {
        assert!(PublicKey::from_base64(PUBLIC_KEY).is_ok());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn only_an_app_bundle_outside_translocation_updates() {
        let app = Path::new("/Applications/Odeion.app/Contents/MacOS/odeion");
        assert_eq!(installed_at(app), Ok(PathBuf::from("/Applications/Odeion.app")));
        assert!(installed_at(Path::new("/Users/me/odeion/target/release/odeion")).unwrap_err().contains("from source"));
        let moved = Path::new("/private/var/folders/x/T/AppTranslocation/ABC/d/Odeion.app/Contents/MacOS/odeion");
        assert!(installed_at(moved).unwrap_err().contains("Move Odeion to Applications"));
    }

    /// The new copy takes the old one's place and name, and nothing is left beside it.
    #[test]
    #[cfg(any(target_os = "macos", windows))]
    fn an_update_takes_the_installed_copys_place() {
        let dir = std::env::temp_dir().join(format!("odeion-update-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let source = dir.join("source");
        // The program in the copy: the copy itself on Windows, inside the bundle on macOS.
        let program = |copy: &Path| if cfg!(windows) { copy.to_path_buf() } else { copy.join("Contents/MacOS/odeion") };
        let write = |copy: &Path, text: &str| {
            let file = program(copy);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, text).unwrap();
        };
        write(&source.join(UNPACKED), "new");
        let zip = dir.join("update.zip");
        let zipped = if cfg!(windows) {
            windows_tar().arg("-a").arg("-cf").arg(&zip).arg("-C").arg(&source).arg(UNPACKED).status()
        } else {
            Command::new("ditto").args(["-c", "-k", "--keepParent"]).arg(source.join(UNPACKED)).arg(&zip).status()
        };
        assert!(zipped.unwrap().success());
        // Installed under a name of its own.
        let at = dir.join(if cfg!(windows) { "odeion.exe" } else { "My Odeion.app" });
        write(&at, "old");

        put_in_place(&fs::read(&zip).unwrap(), &at).unwrap();
        assert_eq!(fs::read_to_string(program(&at)).unwrap(), "new");
        assert!(!dir.join(STAGING).exists());

        // A download that isn't a zip of Odeion leaves the installed copy alone.
        let err = put_in_place(b"not a zip", &at).unwrap_err();
        assert!(err.contains("couldn't put the update in place"), "{err}");
        assert_eq!(fs::read_to_string(program(&at)).unwrap(), "new");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn failures_say_what_to_do() {
        assert_eq!(why(&ureq::Error::StatusCode(404)), "No release is published yet.");
        assert!(why(&ureq::Error::StatusCode(503)).contains("having trouble"));
        assert!(why(&ureq::Error::StatusCode(429)).contains("Try again in a while"));
        assert!(why(&ureq::Error::HostNotFound).contains("Are you online?"));
    }

    /// Asks the real GitHub: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn github_answers() {
        assert!(check().is_ok(), "{:?}", check());
    }
}
