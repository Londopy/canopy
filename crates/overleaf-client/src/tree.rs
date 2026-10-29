//! A project's folder tree with entity ids, from Overleaf's realtime service.
//!
//! The editor talks socket.io 0.9 to the realtime service. Connecting with
//! `?projectId=` makes the server join the project and send the whole tree
//! as a `joinProjectResponse` event; we read that one message and hang up.

use crate::{Error, Server};
use futures_util::{SinkExt, StreamExt};
use reqwest::header::{HeaderMap, HeaderValue, COOKIE, ORIGIN, SET_COOKIE};
use serde::Deserialize;
use std::time::Duration;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

/// What an entity is, as Overleaf's routes name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityKind {
    /// An editable text document.
    Doc,
    /// A binary or uploaded file.
    File,
    /// A folder.
    Folder,
}

impl EntityKind {
    /// The name used in URLs: `doc`, `file` or `folder`.
    pub fn as_str(self) -> &'static str {
        match self {
            EntityKind::Doc => "doc",
            EntityKind::File => "file",
            EntityKind::Folder => "folder",
        }
    }
}

/// A doc or file in a folder.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Leaf {
    /// The entity id.
    #[serde(rename = "_id")]
    pub id: String,
    /// The file name.
    pub name: String,
}

/// A folder and everything under it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Folder {
    /// The folder id. The root folder's id is what uploads are addressed to.
    #[serde(rename = "_id")]
    pub id: String,
    /// The folder name (`rootFolder` for the root).
    #[serde(default)]
    pub name: String,
    /// Subfolders.
    #[serde(default)]
    pub folders: Vec<Folder>,
    /// Editable text documents.
    #[serde(default)]
    pub docs: Vec<Leaf>,
    /// Binary and uploaded files.
    #[serde(rename = "fileRefs", default)]
    pub files: Vec<Leaf>,
}

/// One entry of [`Folder::index`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Indexed {
    /// Relative path, `/`-separated, no leading slash.
    pub path: String,
    /// Doc, file or folder.
    pub kind: EntityKind,
    /// The entity id.
    pub id: String,
}

impl Folder {
    /// Every doc, file and folder under this one, with relative paths.
    pub fn index(&self) -> Vec<Indexed> {
        let mut out = Vec::new();
        self.walk("", &mut out);
        out
    }

    fn walk(&self, prefix: &str, out: &mut Vec<Indexed>) {
        let join = |name: &str| {
            if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}/{name}")
            }
        };
        for d in &self.docs {
            out.push(Indexed {
                path: join(&d.name),
                kind: EntityKind::Doc,
                id: d.id.clone(),
            });
        }
        for f in &self.files {
            out.push(Indexed {
                path: join(&f.name),
                kind: EntityKind::File,
                id: f.id.clone(),
            });
        }
        for sub in &self.folders {
            let path = join(&sub.name);
            out.push(Indexed {
                path: path.clone(),
                kind: EntityKind::Folder,
                id: sub.id.clone(),
            });
            sub.walk(&path, out);
        }
    }

    /// The entry at `path` (relative, `/`-separated), if any.
    pub fn find(&self, path: &str) -> Option<Indexed> {
        let path = path.trim_matches('/');
        self.index().into_iter().find(|e| e.path == path)
    }
}

#[derive(Deserialize)]
struct Event {
    name: String,
    #[serde(default)]
    args: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct JoinResponse {
    project: JoinedProject,
}

#[derive(Deserialize)]
struct JoinedProject {
    #[serde(rename = "rootFolder")]
    root_folder: Vec<Folder>,
}

/// What a socket.io 0.9 frame means to us.
#[derive(Debug, PartialEq)]
enum Frame {
    Heartbeat,
    Tree(Box<Folder>),
    Rejected(String),
    Other,
}

// frames look like "type:id:endpoint:data"; events are type 5
fn parse_frame(text: &str) -> Result<Frame, Error> {
    if text.starts_with("2::") {
        return Ok(Frame::Heartbeat);
    }
    let Some(data) = text.strip_prefix("5:::") else {
        return Ok(Frame::Other);
    };
    let ev: Event = serde_json::from_str(data).map_err(Error::protocol)?;
    match ev.name.as_str() {
        "joinProjectResponse" => {
            let arg = ev.args.into_iter().next().unwrap_or_default();
            let mut resp: JoinResponse = serde_json::from_value(arg).map_err(Error::protocol)?;
            let root = resp
                .project
                .root_folder
                .drain(..)
                .next()
                .ok_or_else(|| Error::Protocol("the project has no root folder".into()))?;
            Ok(Frame::Tree(Box::new(root)))
        }
        "connectionRejected" => {
            let why = ev
                .args
                .first()
                .and_then(|a| a.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("no reason given")
                .to_string();
            Ok(Frame::Rejected(why))
        }
        _ => Ok(Frame::Other),
    }
}

// `cookie` plus every name=value a response set, replacing same-named ones
fn with_set_cookies(cookie: &HeaderValue, headers: &HeaderMap) -> HeaderValue {
    let mut pairs: Vec<(String, String)> = cookie
        .to_str()
        .unwrap_or("")
        .split(";")
        .filter_map(|p| {
            let (k, v) = p.trim().split_once('=')?;
            Some((k.to_string(), v.to_string()))
        })
        .collect();
    for set in headers.get_all(SET_COOKIE) {
        let Some((k, v)) = set
            .to_str()
            .ok()
            .and_then(|s| s.split(';').next())
            .and_then(|kv| kv.trim().split_once('='))
        else {
            continue;
        };
        match pairs.iter_mut().find(|(name, _)| name == k) {
            Some(p) => p.1 = v.to_string(),
            None => pairs.push((k.to_string(), v.to_string())),
        }
    }
    let joined = pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("; ");
    let mut out = HeaderValue::from_str(&joined).unwrap_or_else(|_| cookie.clone());
    out.set_sensitive(true);
    out
}

pub(crate) async fn fetch(
    http: &reqwest::Client,
    server: &Server,
    cookie: &HeaderValue,
    project_id: &str,
) -> Result<Folder, Error> {
    let origin = HeaderValue::from_str(server.base_url()).map_err(Error::protocol)?;

    // step 0: what the editor loads first. behind overleaf.com's load balancer
    // it sets a cookie that pins the session to one realtime server; the
    // handshake and the websocket must carry it
    let mut cookie = cookie.clone();
    if let Ok(resp) = http
        .get(server.url("/socket.io/socket.io.js"))
        .header(COOKIE, cookie.clone())
        .header(ORIGIN, origin.clone())
        .send()
        .await
    {
        cookie = with_set_cookies(&cookie, resp.headers());
    }

    // step 1: the socket.io 0.9 handshake hands out a session id
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let url = server.url(&format!("/socket.io/1/?projectId={project_id}&t={stamp}"));
    let resp = http
        .get(url)
        .header(COOKIE, cookie.clone())
        .header(ORIGIN, origin.clone())
        .send()
        .await
        .map_err(Error::network)?;
    cookie = with_set_cookies(&cookie, resp.headers());
    if resp.status().is_redirection() || resp.status().as_u16() == 401 {
        return Err(Error::SignedOut);
    }
    if !resp.status().is_success() {
        return Err(Error::Http(
            resp.status().as_u16(),
            "socket.io handshake".into(),
        ));
    }
    let body = resp.text().await.map_err(Error::network)?;
    let sid = body
        .split(':')
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::Protocol(format!("unexpected handshake reply {body:?}")))?;

    // step 2: the websocket, which joins the project straight away
    let ws_base = server
        .base_url()
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    let ws_url = format!("{ws_base}/socket.io/1/websocket/{sid}?projectId={project_id}");
    let mut req = ws_url.into_client_request().map_err(Error::protocol)?;
    req.headers_mut().insert(COOKIE, cookie.clone());
    req.headers_mut().insert(ORIGIN, origin);
    let (mut ws, _) = tokio::time::timeout(
        Duration::from_secs(20),
        tokio_tungstenite::connect_async(req),
    )
    .await
    .map_err(|_| Error::Network("the realtime service didn't answer".into()))?
    .map_err(Error::network)?;

    let read = async {
        while let Some(msg) = ws.next().await {
            let text = match msg.map_err(Error::network)? {
                Message::Text(t) => t.to_string(),
                Message::Close(_) => break,
                _ => continue,
            };
            match parse_frame(&text)? {
                Frame::Heartbeat => ws
                    .send(Message::text("2::"))
                    .await
                    .map_err(Error::network)?,
                Frame::Tree(root) => return Ok(*root),
                Frame::Rejected(why)
                    if why.contains("restricted") || why.contains("not authorized") =>
                {
                    return Err(Error::SignedOut)
                }
                Frame::Rejected(why) => {
                    return Err(Error::Protocol(format!("realtime refused: {why}")))
                }
                Frame::Other => {}
            }
        }
        Err(Error::Protocol(
            "the realtime service closed before sending the project".into(),
        ))
    };
    let result = tokio::time::timeout(Duration::from_secs(30), read)
        .await
        .map_err(|_| Error::Network("timed out waiting for the project tree".into()))?;
    let _ = ws.close(None).await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const JOIN: &str = r#"5:::{"name":"joinProjectResponse","args":[{"publicId":"P.x","project":{"_id":"p","name":"T","rootFolder":[{"_id":"root","name":"rootFolder","folders":[{"_id":"f1","name":"chapters","folders":[],"docs":[{"_id":"d2","name":"intro.tex"}],"fileRefs":[]}],"docs":[{"_id":"d1","name":"main.tex"}],"fileRefs":[{"_id":"r1","name":"frog.jpg","hash":"x"}]}]},"permissionsLevel":"owner","protocolVersion":2}]}"#;

    #[test]
    fn reads_the_tree() {
        let Frame::Tree(root) = parse_frame(JOIN).unwrap() else {
            panic!("no tree")
        };
        assert_eq!(root.id, "root");
        let paths: Vec<_> = root.index().into_iter().map(|e| (e.path, e.kind)).collect();
        assert_eq!(
            paths,
            vec![
                ("main.tex".into(), EntityKind::Doc),
                ("frog.jpg".into(), EntityKind::File),
                ("chapters".into(), EntityKind::Folder),
                ("chapters/intro.tex".into(), EntityKind::Doc),
            ]
        );
        assert_eq!(root.find("/chapters/intro.tex").unwrap().id, "d2");
        assert!(root.find("nope.tex").is_none());
    }

    #[test]
    fn merges_load_balancer_cookies() {
        let mut h = HeaderMap::new();
        h.append(
            SET_COOKIE,
            HeaderValue::from_static("GCLB=abc; Path=/; HttpOnly"),
        );
        h.append(
            SET_COOKIE,
            HeaderValue::from_static("overleaf.sid=new; Path=/"),
        );
        let c = with_set_cookies(&HeaderValue::from_static("overleaf.sid=old; other=1"), &h);
        assert_eq!(c.to_str().unwrap(), "overleaf.sid=new; other=1; GCLB=abc");
        assert!(c.is_sensitive());
    }

    #[test]
    fn other_frames() {
        assert_eq!(parse_frame("2::").unwrap(), Frame::Heartbeat);
        assert_eq!(parse_frame("1::").unwrap(), Frame::Other);
        assert_eq!(
            parse_frame(r#"5:::{"name":"connectionRejected","args":[{"message":"restricted"}]}"#)
                .unwrap(),
            Frame::Rejected("restricted".into())
        );
        assert_eq!(
            parse_frame(r#"5:::{"name":"clientTracking.clientUpdated","args":[]}"#).unwrap(),
            Frame::Other
        );
        assert!(parse_frame("5:::{not json").is_err());
    }
}
