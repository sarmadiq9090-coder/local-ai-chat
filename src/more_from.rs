//! "More from FastKiwi": the other FastKiwi apps, for the list in Settings
//! (`ui/features/morefrom`, served by `GET /api/app/more-from`).
//!
//! The data is a vendored copy of the site's `data/apps.json` (schema 1) in
//! `ui/features/morefrom/data/`, refreshed by `tools/sync_apps.py` (source commit
//! in `SOURCE.txt`). It is compiled in: nothing is fetched at runtime, and the
//! icons are the vendored files next to it.
//!
//! Rules: Lemon itself is left out; only https://fastkiwi.app/ pages are linked;
//! a Microsoft Store link (`ms-windows-store://pdp/?productid=<id>`) only for a
//! released app whose listing is live; "coming-soon" apps get the page link only.

use serde::{Deserialize, Serialize};

/// This app's `id` in apps.json (also `SELF_ID` in tools/sync_apps.py).
pub const SELF_ID: &str = "lemon";

const APPS_JSON: &str = include_str!("../ui/features/morefrom/data/apps.json");
const SITE_HOST: &str = "fastkiwi.app";
const STORE_PREFIX: &str = "ms-windows-store://pdp/?productid=";
/// UI path of the vendored copy; `icon` in apps.json ("img/x.svg") is relative to it.
const DATA_DIR: &str = "features/morefrom/data/";

#[derive(Deserialize)]
struct Data {
    schema: u32,
    apps: Vec<RawApp>,
}

#[derive(Deserialize)]
struct RawApp {
    id: String,
    status: String,
    name: Text,
    tagline: Text,
    description: Text,
    url: Text,
    store: Option<Listing>,
    icon: String,
}

#[derive(Deserialize)]
struct Listing {
    id: String,
    live: bool,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct Text {
    pub ar: String,
    pub en: String,
}

/// One entry of `GET /api/app/more-from` (see docs/API.md).
#[derive(Serialize, Debug)]
pub struct App {
    pub id: String,
    pub coming_soon: bool,
    pub name: Text,
    pub tagline: Text,
    pub description: Text,
    /// Page on fastkiwi.app per UI language (both pass [`is_site_url`]).
    pub url: Text,
    /// `ms-windows-store://pdp/?productid=<id>`, or null.
    pub store: Option<String>,
    /// UI path of the vendored icon, or null.
    pub icon: Option<String>,
}

/// The list shown in the app: every app in the vendored file except Lemon.
pub fn apps() -> Vec<App> {
    build(APPS_JSON, SELF_ID)
}

/// Parse `json` and keep what may be shown. An unreadable file or another
/// schema gives an empty list; an entry with an unknown status or a page
/// outside fastkiwi.app is dropped.
fn build(json: &str, self_id: &str) -> Vec<App> {
    let Ok(data) = serde_json::from_str::<Data>(json) else {
        return Vec::new();
    };
    if data.schema != 1 {
        return Vec::new();
    }
    data.apps
        .into_iter()
        .filter(|a| a.id != self_id)
        .filter_map(|a| {
            let coming_soon = match a.status.as_str() {
                "released" => false,
                "coming-soon" => true,
                _ => return None,
            };
            if !is_site_url(&a.url.ar) || !is_site_url(&a.url.en) {
                return None;
            }
            let store = a
                .store
                .filter(|s| s.live && !coming_soon)
                .and_then(|s| store_uri(&s.id));
            let icon = icon_path(&a.icon);
            Some(App {
                id: a.id,
                coming_soon,
                name: a.name,
                tagline: a.tagline,
                description: a.description,
                url: a.url,
                store,
                icon,
            })
        })
        .collect()
}

/// An `https://fastkiwi.app/...` URL: no other host, port, or user info.
pub fn is_site_url(url: &str) -> bool {
    reqwest::Url::parse(url).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str() == Some(SITE_HOST)
            && u.port().is_none()
            && u.username().is_empty()
            && u.password().is_none()
    })
}

/// Microsoft Store product ids are 12 characters, digits and capital letters.
fn is_store_id(id: &str) -> bool {
    id.len() == 12
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase())
}

fn store_uri(id: &str) -> Option<String> {
    is_store_id(id).then(|| format!("{STORE_PREFIX}{id}"))
}

/// Exactly `ms-windows-store://pdp/?productid=<id>` (used by src/window.rs to
/// let this one custom scheme leave the app).
pub fn is_store_uri(url: &str) -> bool {
    url.strip_prefix(STORE_PREFIX).is_some_and(is_store_id)
}

/// `img/<name>.svg|png` → the vendored file's UI path.
fn icon_path(icon: &str) -> Option<String> {
    let name = icon.strip_prefix("img/")?;
    let (stem, ext) = name.rsplit_once('.')?;
    let ok = !stem.is_empty()
        && stem
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && matches!(ext, "svg" | "png");
    ok.then(|| format!("{DATA_DIR}{icon}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_ids() -> Vec<String> {
        let data: Data = serde_json::from_str(APPS_JSON).expect("vendored apps.json parses");
        assert_eq!(data.schema, 1);
        data.apps.into_iter().map(|a| a.id).collect()
    }

    #[test]
    fn vendored_file_lists_every_other_app_and_not_lemon() {
        let ids = raw_ids();
        assert!(ids.iter().any(|id| id == SELF_ID), "apps.json has Lemon");
        let shown: Vec<String> = apps().into_iter().map(|a| a.id).collect();
        assert!(!shown.iter().any(|id| id == SELF_ID), "{shown:?}");
        // Nothing else is dropped: every other entry is valid.
        let others: Vec<String> = ids.into_iter().filter(|id| id != SELF_ID).collect();
        assert_eq!(shown, others);
        assert!(shown.len() >= 4, "{shown:?}");
    }

    #[test]
    fn vendored_links_are_whitelisted_and_coming_soon_has_no_store_link() {
        for app in apps() {
            assert!(is_site_url(&app.url.ar), "{}: {}", app.id, app.url.ar);
            assert!(is_site_url(&app.url.en), "{}: {}", app.id, app.url.en);
            if app.coming_soon {
                assert_eq!(app.store, None, "{}", app.id);
            }
            if let Some(store) = &app.store {
                assert!(is_store_uri(store), "{}: {store}", app.id);
            }
            for text in [&app.name, &app.tagline, &app.description] {
                assert!(!text.ar.is_empty() && !text.en.is_empty(), "{}", app.id);
            }
        }
        let by_id = |id: &str| apps().into_iter().find(|a| a.id == id).unwrap();
        // Remember Me is live in the Store; InstaFind's listing is not live yet.
        assert_eq!(
            by_id("remember-me").store.as_deref(),
            Some("ms-windows-store://pdp/?productid=9NGB1F7T7JGN")
        );
        assert_eq!(by_id("instafind").store, None);
        let convert = by_id("instaconvert");
        assert!(convert.coming_soon);
        assert_eq!(convert.store, None);
    }

    #[test]
    fn vendored_icons_exist() {
        let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
        for app in apps() {
            let icon = app.icon.unwrap_or_else(|| panic!("{} has an icon", app.id));
            assert!(ui.join(&icon).is_file(), "missing {icon}");
        }
    }

    /// Synthetic data: the filtering rules themselves.
    #[test]
    fn rules_on_synthetic_data() {
        let entry = |id: &str, status: &str, url: &str, store: &str| {
            format!(
                r#"{{"id":"{id}","status":"{status}","name":{{"ar":"س","en":"N"}},
                "tagline":{{"ar":"س","en":"T"}},"description":{{"ar":"س","en":"D"}},
                "url":{{"ar":"{url}","en":"{url}"}},"store":{store},"winget":null,
                "icon":"img/{id}.svg"}}"#
            )
        };
        let live = r#"{"id":"9ABCDEFGHJ12","live":true}"#;
        let json = format!(
            r#"{{"schema":1,"apps":[{}]}}"#,
            [
                entry("me", "released", "https://fastkiwi.app/me/", live),
                entry("soon", "coming-soon", "https://fastkiwi.app/soon/", live),
                entry("live", "released", "https://fastkiwi.app/live/", live),
                entry(
                    "unlisted",
                    "released",
                    "https://fastkiwi.app/u/",
                    r#"{"id":"9ABCDEFGHJ12","live":false}"#
                ),
                entry(
                    "badid",
                    "released",
                    "https://fastkiwi.app/b/",
                    r#"{"id":"9abc&x=1","live":true}"#
                ),
                entry("http", "released", "http://fastkiwi.app/h/", "null"),
                entry("evil", "released", "https://fastkiwi.app.evil.com/", "null"),
                entry("odd", "beta", "https://fastkiwi.app/o/", "null"),
            ]
            .join(",")
        );
        let list = build(&json, "me");
        let ids: Vec<&str> = list.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["soon", "live", "unlisted", "badid"]);
        assert!(list[0].coming_soon && list[0].store.is_none());
        assert_eq!(
            list[1].store.as_deref(),
            Some("ms-windows-store://pdp/?productid=9ABCDEFGHJ12")
        );
        assert_eq!(list[2].store, None);
        assert_eq!(list[3].store, None);
        assert_eq!(
            list[1].icon.as_deref(),
            Some("features/morefrom/data/img/live.svg")
        );

        assert!(build("not json", "me").is_empty());
        assert!(build(r#"{"schema":2,"apps":[]}"#, "me").is_empty());
    }

    #[test]
    fn url_whitelist() {
        assert!(is_site_url("https://fastkiwi.app/"));
        assert!(is_site_url(
            "https://fastkiwi.app/ar/downloads/#instaconvert"
        ));
        for bad in [
            "http://fastkiwi.app/",
            "https://fastkiwi.app.evil.com/",
            "https://evil.com/?https://fastkiwi.app/",
            "https://user@fastkiwi.app/",
            "https://fastkiwi.app:8443/",
            "https://www.fastkiwi.app/",
            "javascript:alert(1)",
            "ms-windows-store://pdp/?productid=9NGB1F7T7JGN",
            "",
        ] {
            assert!(!is_site_url(bad), "{bad}");
        }
        assert!(is_store_uri(
            "ms-windows-store://pdp/?productid=9NGB1F7T7JGN"
        ));
        for bad in [
            "ms-windows-store://pdp/?productid=9ngb1f7t7jgn",
            "ms-windows-store://pdp/?productid=9NGB1F7T7JG",
            "ms-windows-store://pdp/?productid=9NGB1F7T7JGN&cid=x",
            "ms-windows-store://pdp/?productid=9NGB1F7T7JGN,/select",
            "ms-windows-store://review/?productid=9NGB1F7T7JGN",
            "MS-WINDOWS-STORE://pdp/?productid=9NGB1F7T7JGN",
            "https://fastkiwi.app/",
        ] {
            assert!(!is_store_uri(bad), "{bad}");
        }
        assert_eq!(
            icon_path("img/a-b1.png").as_deref(),
            Some("features/morefrom/data/img/a-b1.png")
        );
        for bad in [
            "img/../x.svg",
            "img/x.js",
            "x.svg",
            "img/.svg",
            "img/A.svg",
            "img/a/b.svg",
        ] {
            assert_eq!(icon_path(bad), None, "{bad}");
        }
    }
}
