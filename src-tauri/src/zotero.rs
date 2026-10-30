// zotero.rs
// citations from the Zotero library on this computer: search it, and add the
// picked entries to the project's .bib file. Zotero answers on
// 127.0.0.1:23119. with the Better BibTeX plugin, search and export go
// through its JSON-RPC (and its citation keys); without it, through Zotero
// 7's own local API, which has to be switched on in Zotero's settings
// (Settings > Advanced > "Allow other applications on this computer to
// communicate with Zotero").
//
// only in the full build.

use crate::AppState;
use regex::Regex;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::LazyLock;
use std::time::Duration;
use tauri::{AppHandle, Manager};

const BASE: &str = "http://127.0.0.1:23119";

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

fn client() -> Result<reqwest::Client, String> {
    // reqwest is built without a crypto provider of its own; a second
    // install only reports it's already there
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(20))
        // a redirect could take the search, or the library, elsewhere
        .redirect(reqwest::redirect::Policy::none())
        // never anywhere but this computer
        .no_proxy()
        .build()
        .map_err(err)
}

/// What answers on Zotero's port.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    running: bool,
    better_bibtex: bool,
    local_api: bool,
}

/// One library item, for the picker.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    // Zotero's item key, used to export it without Better BibTeX
    key: String,
    // the citation key, when Better BibTeX knows it
    citekey: Option<String>,
    title: String,
    creators: String,
    year: String,
}

async fn bbt(
    c: &reqwest::Client,
    base: &str,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let r: Value = c
        .post(format!("{base}/better-bibtex/json-rpc"))
        .json(&json!({"jsonrpc": "2.0", "method": method, "params": params, "id": 1}))
        .send()
        .await
        .map_err(err)?
        .error_for_status()
        .map_err(err)?
        .json()
        .await
        .map_err(err)?;
    if let Some(e) = r.get("error") {
        return Err(format!(
            "Better BibTeX: {}",
            e.get("message").and_then(|m| m.as_str()).unwrap_or("error")
        ));
    }
    Ok(r.get("result").cloned().unwrap_or(Value::Null))
}

pub async fn status_at(base: &str) -> Status {
    let Ok(c) = client() else {
        return Status {
            running: false,
            better_bibtex: false,
            local_api: false,
        };
    };
    let running = c
        .get(format!("{base}/connector/ping"))
        .send()
        .await
        .is_ok_and(|r| r.status().is_success());
    if !running {
        return Status {
            running,
            better_bibtex: false,
            local_api: false,
        };
    }
    let better_bibtex = bbt(&c, base, "api.ready", json!([])).await.is_ok();
    let local_api = c
        .get(format!("{base}/api/users/0/items?limit=1"))
        .send()
        .await
        .is_ok_and(|r| r.status().is_success());
    Status {
        running,
        better_bibtex,
        local_api,
    }
}

fn names(creators: Option<&Value>) -> String {
    let list: Vec<String> = creators
        .and_then(|c| c.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    p.get("lastName")
                        .or_else(|| p.get("family"))
                        .or_else(|| p.get("name"))
                        .or_else(|| p.get("literal"))
                        .and_then(|v| v.as_str())
                        .map(String::from)
                })
                .collect()
        })
        .unwrap_or_default();
    match list.len() {
        0 => String::new(),
        1 | 2 => list.join(" and "),
        _ => format!("{} et al.", list[0]),
    }
}

fn year(date: &str) -> String {
    static YEAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(\d{4})\b").expect("regex"));
    YEAR.captures(date)
        .map(|c| c[1].to_string())
        .unwrap_or_default()
}

// Better BibTeX's item.search gives CSL-JSON with a citekey
fn from_bbt(v: &Value) -> Option<Item> {
    let citekey = v
        .get("citekey")
        .or_else(|| v.get("citation-key"))
        .and_then(|k| k.as_str())?;
    let issued = v
        .pointer("/issued/date-parts/0/0")
        .map(|y| y.to_string())
        .unwrap_or_default();
    Some(Item {
        key: v
            .get("itemKey")
            .and_then(|k| k.as_str())
            .unwrap_or(citekey)
            .to_string(),
        citekey: Some(citekey.to_string()),
        title: v
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string(),
        creators: names(v.get("author")),
        year: issued.trim_matches('"').to_string(),
    })
}

// the local API's JSON: { key, data: { title, creators, date, itemType } }
fn from_local(v: &Value) -> Option<Item> {
    let data = v.get("data")?;
    let kind = data.get("itemType").and_then(|t| t.as_str()).unwrap_or("");
    if matches!(kind, "attachment" | "note" | "annotation") {
        return None;
    }
    Some(Item {
        key: v.get("key")?.as_str()?.to_string(),
        citekey: None,
        title: data
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string(),
        creators: names(data.get("creators")),
        year: year(data.get("date").and_then(|d| d.as_str()).unwrap_or("")),
    })
}

pub async fn search_at(base: &str, q: &str) -> Result<Vec<Item>, String> {
    let c = client()?;
    let s = status_at(base).await;
    if !s.running {
        return Err("Zotero isn't running. Start it and try again.".into());
    }
    if s.better_bibtex {
        let r = bbt(&c, base, "item.search", json!([q])).await?;
        return Ok(r
            .as_array()
            .map(|a| a.iter().filter_map(from_bbt).take(50).collect())
            .unwrap_or_default());
    }
    if !s.local_api {
        return Err("Zotero is running but doesn't let other apps read its library. In Zotero, open Settings > Advanced and turn on \"Allow other applications on this computer to communicate with Zotero\".".into());
    }
    let r: Value = c
        .get(
            reqwest::Url::parse_with_params(
                &format!("{base}/api/users/0/items"),
                &[("q", q), ("limit", "50"), ("format", "json")],
            )
            .map_err(err)?,
        )
        .send()
        .await
        .map_err(err)?
        .error_for_status()
        .map_err(err)?
        .json()
        .await
        .map_err(err)?;
    Ok(r.as_array()
        .map(|a| a.iter().filter_map(from_local).collect())
        .unwrap_or_default())
}

static ENTRY_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\u{feff}?\s*@\w+\s*\{\s*([^,\s]+)\s*,").expect("regex"));

/// The citation keys a .bib text defines.
pub fn bib_keys(bib: &str) -> Vec<String> {
    ENTRY_KEY
        .captures_iter(bib)
        .map(|c| c[1].to_string())
        .collect()
}

// splits exported BibTeX into (key, entry text) pairs
fn entries(bib: &str) -> Vec<(String, String)> {
    let starts: Vec<(usize, String)> = ENTRY_KEY
        .captures_iter(bib)
        .filter_map(|c| Some((c.get(0)?.start(), c[1].to_string())))
        .collect();
    starts
        .iter()
        .enumerate()
        .map(|(i, (at, key))| {
            let end = starts.get(i + 1).map(|(n, _)| *n).unwrap_or(bib.len());
            (key.clone(), bib[*at..end].trim().to_string())
        })
        .collect()
}

pub async fn export_at(base: &str, items: &[Item]) -> Result<String, String> {
    let c = client()?;
    let s = status_at(base).await;
    if s.better_bibtex && items.iter().all(|i| i.citekey.is_some()) {
        let keys: Vec<&str> = items.iter().filter_map(|i| i.citekey.as_deref()).collect();
        let r = bbt(&c, base, "item.export", json!([keys, "Better BibLaTeX"])).await?;
        // BBT returns the text, or [status, type, text] in older versions
        return Ok(match r {
            Value::String(s) => s,
            Value::Array(a) => a.last().and_then(|v| v.as_str()).unwrap_or("").to_string(),
            _ => String::new(),
        });
    }
    let keys: Vec<&str> = items.iter().map(|i| i.key.as_str()).collect();
    c.get(
        reqwest::Url::parse_with_params(
            &format!("{base}/api/users/0/items"),
            &[("itemKey", keys.join(",").as_str()), ("format", "biblatex")],
        )
        .map_err(err)?,
    )
    .send()
    .await
    .map_err(err)?
    .error_for_status()
    .map_err(err)?
    .text()
    .await
    .map_err(err)
}

/// The text a .bib file gets: what it had, plus the exported entries whose
/// keys it doesn't define yet.
pub fn merge_bib(existing: &str, exported: &str) -> (String, Vec<String>) {
    // BibTeX keys are case-insensitive: Knuth84 and knuth84 are one entry
    let have: Vec<String> = bib_keys(existing)
        .iter()
        .map(|k| k.to_lowercase())
        .collect();
    let mut out = existing.trim_end().to_string();
    let mut added = Vec::new();
    for (key, entry) in entries(exported) {
        let lower = key.to_lowercase();
        if have.contains(&lower) || added.iter().any(|a: &String| a.to_lowercase() == lower) {
            continue;
        }
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&entry);
        added.push(key);
    }
    out.push('\n');
    (out, added)
}

#[tauri::command]
pub async fn zotero_status() -> Status {
    status_at(BASE).await
}

#[tauri::command]
pub async fn zotero_search(query: String) -> Result<Vec<Item>, String> {
    search_at(BASE, &query).await
}

/// What adding citations did: the keys to put in \cite{...}, and which of
/// them were new to the .bib file.
#[derive(Serialize)]
pub struct Added {
    keys: Vec<String>,
    new: Vec<String>,
}

/// Exports `items` from Zotero and adds the ones missing to `bib` (a .bib
/// file of the project, created if it doesn't exist).
#[tauri::command]
pub async fn zotero_add(
    app: AppHandle,
    link_id: String,
    bib: String,
    items: Vec<ItemRef>,
) -> Result<Added, String> {
    if !bib.to_ascii_lowercase().ends_with(".bib") {
        return Err("citations go into a .bib file".into());
    }
    let folder = {
        let state = app.state::<AppState>();
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        cfg.links
            .iter()
            .find(|l| l.id == link_id)
            .map(|l| l.folder.clone())
            .ok_or("that project isn't synced to a folder")?
    };
    let items: Vec<Item> = items.into_iter().map(Item::from).collect();
    let exported = export_at(BASE, &items).await?;
    let keys: Vec<String> = entries(&exported).into_iter().map(|(k, _)| k).collect();
    if keys.is_empty() {
        return Err("Zotero didn't export anything for those items".into());
    }
    let target = canopy_sync::resolve(&folder, &bib).map_err(err)?;
    // a missing file starts empty; anything else unreadable (another
    // encoding, a lock) stops here, or the write below would replace it
    let existing = match std::fs::read(&target) {
        Ok(bytes) => String::from_utf8(bytes).map_err(|_| {
            format!("{bib} isn't UTF-8, so Canopy won't rewrite it. Save it as UTF-8 first.")
        })?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("couldn't read {bib}: {e}")),
    };
    let (text, new) = merge_bib(&existing, &exported);
    if !new.is_empty() {
        canopy_sync::write_project_file(&folder, &bib, text.as_bytes()).map_err(err)?;
    }
    Ok(Added { keys, new })
}

/// An item as the picker sends it back.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ItemRef {
    key: String,
    citekey: Option<String>,
}

impl From<ItemRef> for Item {
    fn from(r: ItemRef) -> Self {
        Item {
            key: r.key,
            citekey: r.citekey,
            title: String::new(),
            creators: String::new(),
            year: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    // a stand-in for Zotero: answers each request by its path
    fn fake_zotero(routes: Vec<(&'static str, &'static str)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten().take(40) {
                let mut s = stream;
                let mut buf = vec![0u8; 16384];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                let body = routes
                    .iter()
                    .find(|(p, _)| path.starts_with(p))
                    .map(|(_, b)| *b);
                let resp = match body {
                    Some(b) => format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
                        b.len()
                    ),
                    None => {
                        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .into()
                    }
                };
                let _ = s.write_all(resp.as_bytes());
            }
        });
        format!("http://{addr}")
    }

    fn block<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(f)
    }

    const LOCAL_ITEMS: &str = r#"[
        {"key":"ABCD1234","data":{"itemType":"book","title":"The TeXbook","creators":[{"lastName":"Knuth","firstName":"Donald"}],"date":"1984"}},
        {"key":"NOTE0001","data":{"itemType":"note","note":"x"}}
    ]"#;
    const LOCAL_BIB: &str = "@book{knuthTeXbook1984,\n  title = {The {TeXbook}},\n  author = {Knuth, Donald},\n  date = {1984}\n}\n";

    #[test]
    fn searches_and_exports_through_the_local_api() {
        let base = fake_zotero(vec![
            ("/connector/ping", "Zotero is running"),
            ("/api/users/0/items?limit=1", "[]"),
            ("/api/users/0/items?itemKey", LOCAL_BIB),
            ("/api/users/0/items?q", LOCAL_ITEMS),
        ]);
        let s = block(status_at(&base));
        assert_eq!(
            s,
            Status {
                running: true,
                better_bibtex: false,
                local_api: true
            }
        );
        let items = block(search_at(&base, "knuth")).unwrap();
        assert_eq!(items.len(), 1, "notes are left out");
        assert_eq!(items[0].title, "The TeXbook");
        assert_eq!(items[0].creators, "Knuth");
        assert_eq!(items[0].year, "1984");
        let bib = block(export_at(&base, &items)).unwrap();
        assert_eq!(bib_keys(&bib), vec!["knuthTeXbook1984"]);
    }

    #[test]
    fn says_what_to_switch_on_when_the_local_api_is_off() {
        let base = fake_zotero(vec![("/connector/ping", "Zotero is running")]);
        let e = block(search_at(&base, "x")).unwrap_err();
        assert!(e.contains("Allow other applications"), "{e}");
    }

    #[test]
    fn says_when_zotero_isnt_running() {
        // nothing listens on this port
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        drop(l);
        assert!(block(search_at(&base, "x"))
            .unwrap_err()
            .contains("isn't running"));
    }

    #[test]
    fn uses_better_bibtex_when_its_there() {
        let base = fake_zotero(vec![
            ("/connector/ping", "ok"),
            ("/api/users/0/items?limit=1", "[]"),
            (
                "/better-bibtex/json-rpc",
                r#"{"jsonrpc":"2.0","id":1,"result":[{"citekey":"knuth1984","title":"The TeXbook","author":[{"family":"Knuth"}],"issued":{"date-parts":[[1984]]}}]}"#,
            ),
        ]);
        assert!(block(status_at(&base)).better_bibtex);
        let items = block(search_at(&base, "knuth")).unwrap();
        assert_eq!(items[0].citekey.as_deref(), Some("knuth1984"));
        assert_eq!(items[0].year, "1984");
    }

    #[test]
    fn merges_only_new_entries() {
        let existing = "@article{old2020,\n  title = {Old}\n}\n";
        let exported = "@book{old2020,\n  title = {Dup}\n}\n\n@book{new2024,\n  title = {New}\n}\n";
        let (text, added) = merge_bib(existing, exported);
        assert_eq!(added, vec!["new2024"]);
        assert_eq!(bib_keys(&text), vec!["old2020", "new2024"]);
        assert!(text.contains("title = {Old}") && !text.contains("{Dup}"));
        // a byte-order mark and a different case don't make a new entry
        let (_, added) = merge_bib("\u{feff}@book{NEW2024,\n title = {x}\n}\n", exported);
        assert_eq!(added, vec!["old2020"]);
        let (from_empty, added) = merge_bib("", exported);
        assert_eq!(added, vec!["old2020", "new2024"]);
        assert!(from_empty.starts_with("@book{old2020"));
    }
}
