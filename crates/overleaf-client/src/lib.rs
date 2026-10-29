//! Everything Canopy knows about talking to an Overleaf server.
//!
//! Overleaf has no public API, so this crate is the one place that knows the
//! endpoints the web app itself uses. When Overleaf changes something, the fix
//! belongs here and nowhere else.

mod client;
mod tree;

pub use client::{Client, Entity, ProjectSummary};
pub use tree::{EntityKind, Folder, Indexed, Leaf};

use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::{IpAddr, Ipv6Addr};

/// Why a request to the server failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The session is missing or expired: sign in on the server's page again.
    #[error("you're signed out of this server; open the account and sign in again")]
    SignedOut,
    /// The server answered with an error status.
    #[error("the server said {0}: {1}")]
    Http(u16, String),
    /// The server couldn't be reached.
    #[error("couldn't reach the server: {0}")]
    Network(String),
    /// The server answered with something Canopy doesn't understand, most
    /// likely because Overleaf changed.
    #[error("unexpected answer from the server: {0}")]
    Protocol(String),
}

impl Error {
    pub(crate) fn network(e: impl fmt::Display) -> Self {
        Error::Network(e.to_string())
    }

    pub(crate) fn protocol(e: impl fmt::Display) -> Self {
        Error::Protocol(e.to_string())
    }
}

/// The hosted service, used when an account doesn't name a server.
pub const OVERLEAF_COM: &str = "https://www.overleaf.com";

/// Why a server address was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ServerError {
    /// The address was empty or only whitespace.
    #[error("the server address is empty")]
    Empty,
    /// The address used something other than http or https.
    #[error("the server address must start with http:// or https:// (got {0:?})")]
    Scheme(String),
    /// The address had no host after the scheme.
    #[error("the server address has no host")]
    NoHost,
    /// The host isn't a valid name or IP address.
    #[error("{0:?} isn't a valid server name")]
    BadHost(String),
    /// The port isn't a number from 1 to 65535.
    #[error("{0:?} isn't a valid port")]
    BadPort(String),
    /// Plain http to a machine other than this one would send the session cookie in the clear.
    #[error("plain http is only allowed for localhost; use https for {0}")]
    InsecureRemote(String),
}

/// An Overleaf server: overleaf.com or a self-hosted Community Edition / Server Pro.
///
/// The base URL is kept normalized (lowercase scheme and host, no trailing
/// slash, no path, query or fragment), so two spellings of the same server
/// compare equal and endpoints can be joined with a plain `format!`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Server {
    base: String,
}

impl Server {
    /// overleaf.com.
    pub fn overleaf_com() -> Self {
        Self {
            base: OVERLEAF_COM.to_string(),
        }
    }

    /// Parses and normalizes a server address typed by the user.
    ///
    /// A bare host (`overleaf.example.org`) gets `https://`. Plain `http://`
    /// is accepted only for localhost, since the session cookie would
    /// otherwise cross the network unencrypted.
    pub fn parse(input: &str) -> Result<Self, ServerError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ServerError::Empty);
        }
        // a "://" only names the scheme when it comes before any path, query or
        // fragment; `tex.example.org/p?next=https://x` is a bare host
        let first_delim = input.find(['/', '?', '#']).unwrap_or(input.len());
        let (scheme, rest) = match input.find("://") {
            Some(i) if i < first_delim => (input[..i].to_ascii_lowercase(), &input[i + 3..]),
            _ => ("https".to_string(), input),
        };
        if scheme != "http" && scheme != "https" {
            return Err(ServerError::Scheme(scheme));
        }
        // the authority ends at the first path, query or fragment character
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        // drop any user:pass@ prefix; credentials never belong in the stored URL
        let authority = authority
            .rsplit('@')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let (host, port) = split_host_port(&authority)?;
        let default_port = if scheme == "https" { 443 } else { 80 };
        if scheme == "http" && !is_loopback(&host) {
            return Err(ServerError::InsecureRemote(host));
        }
        // overleaf.com redirects the bare domain to www, so store the form it serves
        let host = if host == "overleaf.com" {
            "www.overleaf.com".to_string()
        } else {
            host
        };
        let host = if host.contains(':') {
            format!("[{host}]")
        } else {
            host
        };
        let base = match port {
            Some(p) if p != default_port => format!("{scheme}://{host}:{p}"),
            _ => format!("{scheme}://{host}"),
        };
        Ok(Self { base })
    }

    /// The normalized base URL, e.g. `https://www.overleaf.com`.
    pub fn base_url(&self) -> &str {
        &self.base
    }

    /// The host part, e.g. `www.overleaf.com` or `localhost:8080`.
    pub fn host(&self) -> &str {
        self.base
            .split_once("://")
            .map(|(_, h)| h)
            .unwrap_or(&self.base)
    }

    /// True for the hosted overleaf.com service.
    pub fn is_overleaf_com(&self) -> bool {
        self.base == OVERLEAF_COM
    }

    /// Joins a path onto the base URL. `path` should start with `/`.
    pub fn url(&self, path: &str) -> String {
        if path.starts_with('/') {
            format!("{}{path}", self.base)
        } else {
            format!("{}/{path}", self.base)
        }
    }

    /// The sign-in page, opened in a webview so the user types their password
    /// into Overleaf itself and Canopy only ever sees the session cookie.
    pub fn login_url(&self) -> String {
        self.url("/login")
    }

    /// The project dashboard.
    pub fn projects_url(&self) -> String {
        self.url("/project")
    }
}

// "host", "host:port", "[v6]" or "[v6]:port" -> (host without brackets, port)
fn split_host_port(authority: &str) -> Result<(String, Option<u16>), ServerError> {
    if authority.is_empty() {
        return Err(ServerError::NoHost);
    }
    let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
        let (v6, after) = rest.split_once(']').ok_or_else(|| bad_host(authority))?;
        if v6.parse::<Ipv6Addr>().is_err() {
            return Err(bad_host(authority));
        }
        match after {
            "" => (v6, None),
            p => match p.strip_prefix(':') {
                Some(p) => (v6, Some(p)),
                None => return Err(bad_host(authority)),
            },
        }
    } else {
        match authority.split_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (authority, None),
        }
    };
    if host.is_empty() {
        return Err(ServerError::NoHost);
    }
    if !host.contains(':') && !valid_hostname(host) {
        return Err(bad_host(host));
    }
    let port = match port {
        None => None,
        Some(p) => match p.parse::<u16>() {
            Ok(n) if n > 0 && p.bytes().all(|b| b.is_ascii_digit()) => Some(n),
            _ => return Err(ServerError::BadPort(p.to_string())),
        },
    };
    Ok((host.to_string(), port))
}

fn bad_host(h: &str) -> ServerError {
    ServerError::BadHost(h.to_string())
}

// DNS names and IPv4 literals: dot-separated labels of letters, digits and
// hyphens, 1-63 characters, no hyphen at either end
fn valid_hostname(host: &str) -> bool {
    host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

// this machine only: localhost names and loopback IP literals. a name like
// 127.evil.example is not an address, so it doesn't count
fn is_loopback(host: &str) -> bool {
    host == "localhost"
        || host.ends_with(".localhost")
        || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

impl Default for Server {
    fn default() -> Self {
        Self::overleaf_com()
    }
}

impl fmt::Display for Server {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.base)
    }
}

impl TryFrom<String> for Server {
    type Error = ServerError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::parse(&s)
    }
}

impl From<Server> for String {
    fn from(s: Server) -> String {
        s.base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_host_gets_https() {
        let s = Server::parse("overleaf.example.org").unwrap();
        assert_eq!(s.base_url(), "https://overleaf.example.org");
    }

    #[test]
    fn normalizes_case_path_and_trailing_slash() {
        let s = Server::parse("  HTTPS://Overleaf.Example.org/project/abc?x=1#top ").unwrap();
        assert_eq!(s.base_url(), "https://overleaf.example.org");
        assert_eq!(s, Server::parse("https://overleaf.example.org/").unwrap());
    }

    #[test]
    fn bare_overleaf_com_maps_to_www() {
        assert!(Server::parse("overleaf.com").unwrap().is_overleaf_com());
        assert!(Server::parse("https://www.overleaf.com/project")
            .unwrap()
            .is_overleaf_com());
        assert!(Server::default().is_overleaf_com());
    }

    #[test]
    fn keeps_ports() {
        let s = Server::parse("http://localhost:8080").unwrap();
        assert_eq!(s.base_url(), "http://localhost:8080");
        assert_eq!(s.host(), "localhost:8080");
    }

    #[test]
    fn strips_credentials() {
        let s = Server::parse("https://me:secret@tex.example.org/").unwrap();
        assert_eq!(s.base_url(), "https://tex.example.org");
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(Server::parse("   "), Err(ServerError::Empty));
        assert_eq!(
            Server::parse("ftp://x.org"),
            Err(ServerError::Scheme("ftp".into()))
        );
        assert_eq!(Server::parse("https://"), Err(ServerError::NoHost));
        assert_eq!(Server::parse("https://:443"), Err(ServerError::NoHost));
    }

    #[test]
    fn plain_http_only_for_loopback() {
        for ok in [
            "http://localhost",
            "http://127.0.0.1:80",
            "http://[::1]:8080",
            "http://tex.localhost",
        ] {
            assert!(Server::parse(ok).is_ok(), "{ok}");
        }
        assert_eq!(
            Server::parse("http://tex.example.org"),
            Err(ServerError::InsecureRemote("tex.example.org".into()))
        );
        // names that only look like loopback addresses
        for bad in [
            "http://127.evil.example",
            "http://127.attacker.com:8080",
            "http://localhost.evil.example",
            "http://[::2]",
        ] {
            assert!(
                matches!(Server::parse(bad), Err(ServerError::InsecureRemote(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn validates_host_and_port() {
        assert!(matches!(
            Server::parse("https://exa mple.org"),
            Err(ServerError::BadHost(_))
        ));
        assert!(matches!(
            Server::parse("https://-x.org"),
            Err(ServerError::BadHost(_))
        ));
        assert!(matches!(
            Server::parse("https://a..b"),
            Err(ServerError::BadHost(_))
        ));
        assert!(matches!(
            Server::parse("https://[nope]"),
            Err(ServerError::BadHost(_))
        ));
        assert!(matches!(
            Server::parse("https://x.org:99999"),
            Err(ServerError::BadPort(_))
        ));
        assert!(matches!(
            Server::parse("https://x.org:0"),
            Err(ServerError::BadPort(_))
        ));
        assert!(matches!(
            Server::parse("https://x.org:+80"),
            Err(ServerError::BadPort(_))
        ));
    }

    #[test]
    fn drops_default_ports() {
        assert_eq!(
            Server::parse("example.org:443").unwrap(),
            Server::parse("example.org").unwrap()
        );
        assert_eq!(
            Server::parse("http://localhost:80").unwrap().base_url(),
            "http://localhost"
        );
        assert_eq!(
            Server::parse("https://x.org:8443").unwrap().base_url(),
            "https://x.org:8443"
        );
        assert_eq!(
            Server::parse("http://[::1]:8080").unwrap().base_url(),
            "http://[::1]:8080"
        );
    }

    #[test]
    fn scheme_only_before_the_path() {
        assert_eq!(
            Server::parse("tex.example.org/p?next=https://x")
                .unwrap()
                .base_url(),
            "https://tex.example.org"
        );
    }

    #[test]
    fn builds_urls() {
        let s = Server::overleaf_com();
        assert_eq!(s.login_url(), "https://www.overleaf.com/login");
        assert_eq!(s.projects_url(), "https://www.overleaf.com/project");
        assert_eq!(s.url("project/1"), "https://www.overleaf.com/project/1");
    }

    #[test]
    fn serde_roundtrip_validates() {
        let s = Server::parse("tex.example.org").unwrap();
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "\"https://tex.example.org\"");
        assert_eq!(serde_json::from_str::<Server>(&json).unwrap(), s);
        assert!(serde_json::from_str::<Server>("\"ftp://nope\"").is_err());
    }
}
