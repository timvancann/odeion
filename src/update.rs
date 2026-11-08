//! Checking for a newer Odeion: the latest release on GitHub, asked for only when Check
//! for Updates is chosen in the menu bar. It is the one network request Odeion makes.

use serde::Deserialize;
use std::time::Duration;

const LATEST: &str = "https://api.github.com/repos/timvancann/odeion/releases/latest";

/// A published release: its version and where to get it.
#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    /// The Mac download on a Mac, else the release's page.
    pub download: String,
}

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
    let zip = answer
        .assets
        .iter()
        .find(|a| a.name.starts_with("Odeion-") && a.name.ends_with(".zip"))
        .filter(|_| cfg!(target_os = "macos"))
        .map(|a| a.browser_download_url.clone());
    Ok(Release {
        version: answer.tag_name.trim_start_matches('v').to_string(),
        download: zip.unwrap_or(answer.html_url),
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

/// Asks GitHub, on the calling thread, giving up after ten seconds.
pub fn check() -> Result<Found, String> {
    let agent: ureq::Agent =
        ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(10))).build().into();
    let mut response = agent
        .get(LATEST)
        .header("User-Agent", concat!("Odeion/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| why(&e))?;
    let body = response.body_mut().read_to_string().map_err(|_| "GitHub's answer broke off".to_string())?;
    Ok(found(env!("CARGO_PKG_VERSION"), parse(&body)?))
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
            "assets": [{"name": "notes.txt", "browser_download_url": "https://x/notes.txt"},
                       {"name": "Odeion-0.3.0.zip", "browser_download_url": "https://x/Odeion-0.3.0.zip"}]}"#;
        let release = parse(body).unwrap();
        assert_eq!(release.version, "0.3.0");
        let expected = if cfg!(target_os = "macos") {
            "https://x/Odeion-0.3.0.zip"
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
