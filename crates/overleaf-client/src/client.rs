//! The HTTP side: project list, downloads, uploads and deletes, using the
//! same endpoints Overleaf's own web app calls, with the user's session.

use crate::tree::{self, EntityKind, Folder};
use crate::{Error, Server};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, COOKIE, LOCATION, USER_AGENT};
use reqwest::{Response, StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::Duration;

/// A project as the project list reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSummary {
    /// The project id (a Mongo object id).
    #[serde(rename = "_id")]
    pub id: String,
    /// The project's name.
    pub name: String,
    /// `owner`, `readAndWrite`, `review` or `readOnly`.
    #[serde(rename = "accessLevel", default)]
    pub access_level: String,
}

impl ProjectSummary {
    /// Whether this user can change the project's files.
    pub fn writable(&self) -> bool {
        matches!(self.access_level.as_str(), "owner" | "readAndWrite")
    }
}

/// One file or doc in a project, by path.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Entity {
    /// Absolute in the project, e.g. `/chapters/intro.tex`.
    pub path: String,
    /// `doc` (editable text) or `file` (anything else).
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Deserialize)]
struct ProjectList {
    projects: Vec<ProjectSummary>,
}

#[derive(Deserialize)]
struct EntityList {
    entities: Vec<Entity>,
}

/// A signed-in connection to one Overleaf server.
///
/// It carries the session cookie the user got by signing in on the server's
/// own page; Canopy never sees or stores a password.
pub struct Client {
    http: reqwest::Client,
    server: Server,
    cookie: HeaderValue,
    // the CSRF token from the dashboard, fetched once and reused
    csrf: Mutex<Option<String>>,
}

impl Client {
    /// A client for `server`, sending `cookie` (a `Cookie:` header value,
    /// `name=value; name2=value2`) with every request.
    pub fn new(server: Server, cookie: &str) -> Result<Self, Error> {
        // reqwest is built without a crypto provider of its own (the same
        // build the updater uses); ring is the one compiled in. a second
        // install just reports it's already there
        let _ = rustls::crypto::ring::default_provider().install_default();

        let mut headers = HeaderMap::new();
        let ua = format!(
            "Canopy/{} (+https://github.com/Londopy/canopy)",
            env!("CARGO_PKG_VERSION")
        );
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(&ua).map_err(Error::protocol)?,
        );
        let http = reqwest::Client::builder()
            .default_headers(headers)
            // a redirect to /login means the session ran out; see check()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(15))
            // a stall, not a total: a big project on a slow link can take
            // minutes and must still finish
            .read_timeout(Duration::from_secs(60))
            .build()
            .map_err(Error::network)?;
        let mut cookie = HeaderValue::from_str(cookie).map_err(|_| Error::SignedOut)?;
        cookie.set_sensitive(true);
        Ok(Self {
            http,
            server,
            cookie,
            csrf: Mutex::new(None),
        })
    }

    /// The server this client talks to.
    pub fn server(&self) -> &Server {
        &self.server
    }

    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.http
            .get(self.server.url(path))
            .header(COOKIE, self.cookie.clone())
    }

    // sends a GET, waiting and retrying (twice at most) when the server says
    // it's getting too many requests
    async fn send_get(&self, req: reqwest::RequestBuilder) -> Result<Response, Error> {
        let mut attempt = 0;
        loop {
            let resp = req
                .try_clone()
                .ok_or_else(|| Error::Protocol("request can't be retried".into()))?
                .send()
                .await
                .map_err(Error::network)?;
            if resp.status() == StatusCode::TOO_MANY_REQUESTS && attempt < 2 {
                attempt += 1;
                let wait = resp
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .unwrap_or(5 * attempt)
                    .min(30);
                tokio::time::sleep(Duration::from_secs(wait)).await;
                continue;
            }
            return self.check(resp).await;
        }
    }

    async fn check(&self, resp: Response) -> Result<Response, Error> {
        let status = resp.status();
        if status.is_redirection() {
            let to = resp
                .headers()
                .get(LOCATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            return Err(if to.contains("/login") {
                Error::SignedOut
            } else {
                Error::Http(status.as_u16(), format!("redirected to {to}"))
            });
        }
        if status == StatusCode::UNAUTHORIZED {
            return Err(Error::SignedOut);
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Http(status.as_u16(), truncate(&body, 300)));
        }
        Ok(resp)
    }

    /// The projects this account can see, archived and trashed ones left out.
    pub async fn projects(&self) -> Result<Vec<ProjectSummary>, Error> {
        let list: ProjectList = self
            .send_get(
                self.get("/user/projects")
                    .header(ACCEPT, "application/json"),
            )
            .await?
            .json()
            .await
            .map_err(Error::protocol)?;
        Ok(list.projects)
    }

    /// Every doc and file in a project, by path.
    pub async fn entities(&self, project_id: &str) -> Result<Vec<Entity>, Error> {
        let req = self
            .get(&format!("/project/{}/entities", seg(project_id)?))
            .header(ACCEPT, "application/json");
        let list: EntityList = self
            .send_get(req)
            .await?
            .json()
            .await
            .map_err(Error::protocol)?;
        Ok(list.entities)
    }

    /// The whole project as a zip, the same download as Overleaf's menu.
    pub async fn download_zip(&self, project_id: &str) -> Result<Vec<u8>, Error> {
        let req = self.get(&format!("/project/{}/download/zip", seg(project_id)?));
        let bytes = self
            .send_get(req)
            .await?
            .bytes()
            .await
            .map_err(Error::network)?;
        Ok(bytes.to_vec())
    }

    /// The project's folder tree with entity ids, which deletes need.
    pub async fn tree(&self, project_id: &str) -> Result<Folder, Error> {
        tree::fetch(&self.http, &self.server, &self.cookie, seg(project_id)?).await
    }

    async fn csrf(&self) -> Result<String, Error> {
        if let Some(t) = self.csrf.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            return Ok(t);
        }
        let page = self
            .send_get(self.get("/project"))
            .await?
            .text()
            .await
            .map_err(Error::network)?;
        let token = csrf_from_page(&page)
            .ok_or_else(|| Error::Protocol("no CSRF token on the dashboard".into()))?;
        *self.csrf.lock().unwrap_or_else(|e| e.into_inner()) = Some(token.clone());
        Ok(token)
    }

    /// Uploads one file to `path` (relative, `/`-separated) in the project,
    /// creating folders as needed. An existing doc at that path is updated in
    /// place, the way Overleaf's Dropbox and GitHub sync do it, so it keeps
    /// its history and comments.
    pub async fn upload(
        &self,
        project_id: &str,
        root_folder_id: &str,
        path: &str,
        bytes: Vec<u8>,
    ) -> Result<(), Error> {
        let path = path.trim_start_matches('/');
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        let csrf = self.csrf().await?;
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name(name.clone())
            .mime_str("application/octet-stream")
            .map_err(Error::protocol)?;
        let form = reqwest::multipart::Form::new()
            .text("name", name)
            // overleaf reads "null" as "not in a folder"
            .text(
                "relativePath",
                if path.contains('/') {
                    path.to_string()
                } else {
                    "null".into()
                },
            )
            .part("qqfile", part);
        let url = format!(
            "{}?folder_id={}",
            self.server
                .url(&format!("/project/{}/upload", seg(project_id)?)),
            seg(root_folder_id)?
        );
        let resp = self
            .http
            .post(url)
            .header(COOKIE, self.cookie.clone())
            .header("X-Csrf-Token", csrf)
            .header(ACCEPT, "application/json")
            .multipart(form)
            .send()
            .await
            .map_err(Error::network)?;
        let body: serde_json::Value = self
            .check(resp)
            .await?
            .json()
            .await
            .map_err(Error::protocol)?;
        if body.get("success").and_then(|v| v.as_bool()) == Some(true) {
            Ok(())
        } else {
            Err(Error::Protocol(format!("upload refused: {body}")))
        }
    }

    /// Deletes a doc, file or folder by id.
    pub async fn delete(&self, project_id: &str, kind: EntityKind, id: &str) -> Result<(), Error> {
        let csrf = self.csrf().await?;
        let url = self.server.url(&format!(
            "/project/{}/{}/{}",
            seg(project_id)?,
            kind.as_str(),
            seg(id)?
        ));
        let resp = self
            .http
            .delete(url)
            .header(COOKIE, self.cookie.clone())
            .header("X-Csrf-Token", csrf)
            .send()
            .await
            .map_err(Error::network)?;
        self.check(resp).await.map(|_| ())
    }
}

// ids go into URL paths; overleaf's are 24 hex digits, so anything else is refused
fn seg(id: &str) -> Result<&str, Error> {
    if !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        Ok(id)
    } else {
        Err(Error::Protocol(format!("{id:?} isn't an Overleaf id")))
    }
}

fn csrf_from_page(page: &str) -> Option<String> {
    let at = page.find("name=\"ol-csrfToken\"")?;
    let rest = &page[at..];
    let start = rest.find("content=\"")? + "content=\"".len();
    let end = rest[start..].find('"')?;
    Some(rest[start..start + end].to_string())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut end = max;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &s[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_project_list() {
        let json = r#"{"projects":[{"_id":"6ae2e6a197229a2a6d2f5002","name":"Thesis","accessLevel":"owner"},
            {"_id":"6ae2e6a397229a2a6d2f501c","name":"Shared","accessLevel":"readOnly"}]}"#;
        let list: ProjectList = serde_json::from_str(json).unwrap();
        assert_eq!(list.projects.len(), 2);
        assert!(list.projects[0].writable());
        assert!(!list.projects[1].writable());
    }

    #[test]
    fn parses_entities() {
        let json = r#"{"project_id":"x","entities":[{"path":"/main.tex","type":"doc"},{"path":"/frog.jpg","type":"file"}]}"#;
        let list: EntityList = serde_json::from_str(json).unwrap();
        assert_eq!(
            list.entities[1],
            Entity {
                path: "/frog.jpg".into(),
                kind: "file".into()
            }
        );
    }

    #[test]
    fn finds_the_csrf_token() {
        let page = r#"<meta name="ol-user_id" content="u"><meta name="ol-csrfToken" content="abc-123_x"><meta name="x">"#;
        assert_eq!(csrf_from_page(page).as_deref(), Some("abc-123_x"));
        assert_eq!(csrf_from_page("<html></html>"), None);
    }

    #[test]
    fn ids_must_be_plain() {
        assert!(seg("6ae2e6a197229a2a6d2f5002").is_ok());
        assert!(seg("../user").is_err());
        assert!(seg("a/b").is_err());
        assert!(seg("").is_err());
    }

    #[test]
    fn truncates_on_char_boundaries() {
        assert_eq!(truncate("héllo", 2), "h…");
        assert_eq!(truncate("hi", 10), "hi");
    }
}
