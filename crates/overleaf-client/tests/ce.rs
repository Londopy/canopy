//! Against the local Overleaf CE in dev/overleaf-ce (see its compose.yml):
//!
//!     docker compose -f dev/overleaf-ce/compose.yml up -d
//!     python dev/overleaf-ce/seed.py
//!     cargo test -p canopy-overleaf --test ce -- --ignored
//!
//! Ignored by default so CI and plain `cargo test` don't need a server.

use canopy_overleaf::{Client, EntityKind, Server};
use std::io::Read;

struct Seeded {
    client: Client,
    example: String,
    blank: String,
}

fn seeded() -> Seeded {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../dev/overleaf-ce/.test-user.json"
    );
    let text = std::fs::read_to_string(path).expect("run dev/overleaf-ce/seed.py first");
    let state: serde_json::Value = serde_json::from_str(&text).unwrap();
    let server = Server::parse("http://localhost:8090").unwrap();
    Seeded {
        client: Client::new(server, state["cookie"].as_str().unwrap()).unwrap(),
        example: state["projects"]["example"].as_str().unwrap().to_string(),
        blank: state["projects"]["blank"].as_str().unwrap().to_string(),
    }
}

fn zip_entry(zip: &[u8], name: &str) -> Option<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).ok()?;
    let mut f = archive.by_name(name).ok()?;
    let mut out = Vec::new();
    f.read_to_end(&mut out).ok()?;
    Some(out)
}

#[tokio::test]
#[ignore = "needs the local Overleaf CE"]
async fn lists_downloads_and_reads_the_tree() {
    let s = seeded();
    let projects = s.client.projects().await.unwrap();
    assert!(projects.iter().any(|p| p.id == s.example && p.writable()));

    let entities = s.client.entities(&s.example).await.unwrap();
    assert!(entities
        .iter()
        .any(|e| e.path == "/main.tex" && e.kind == "doc"));

    let zip = s.client.download_zip(&s.example).await.unwrap();
    let main = zip_entry(&zip, "main.tex").expect("main.tex in the zip");
    assert!(String::from_utf8_lossy(&main).contains("\\documentclass"));

    let tree = s.client.tree(&s.example).await.unwrap();
    assert_eq!(tree.find("main.tex").unwrap().kind, EntityKind::Doc);
    assert_eq!(tree.find("frog.jpg").unwrap().kind, EntityKind::File);
}

#[tokio::test]
#[ignore = "needs the local Overleaf CE"]
async fn uploads_replaces_and_deletes() {
    let s = seeded();
    let root = s.client.tree(&s.blank).await.unwrap();

    // a new file in a new folder
    s.client
        .upload(
            &s.blank,
            &root.id,
            "canopy-probe/hello.txt",
            b"hello from canopy\n".to_vec(),
        )
        .await
        .unwrap();
    let tree = s.client.tree(&s.blank).await.unwrap();
    let added = tree
        .find("canopy-probe/hello.txt")
        .expect("uploaded file in the tree");
    let folder = tree.find("canopy-probe").expect("folder made on the way");

    // replacing an existing doc keeps it a doc, with the same id
    let before = tree.find("main.tex").expect("blank project has main.tex");
    let body = format!(
        "\\documentclass{{article}}\n\\begin{{document}}\nreplaced {}\n\\end{{document}}\n",
        std::process::id()
    );
    s.client
        .upload(&s.blank, &root.id, "main.tex", body.clone().into_bytes())
        .await
        .unwrap();
    let after = s
        .client
        .tree(&s.blank)
        .await
        .unwrap()
        .find("main.tex")
        .unwrap();
    assert_eq!(after.kind, EntityKind::Doc);
    assert_eq!(after.id, before.id, "upsert should update the doc in place");
    let zip = s.client.download_zip(&s.blank).await.unwrap();
    assert_eq!(
        String::from_utf8(zip_entry(&zip, "main.tex").unwrap()).unwrap(),
        body
    );

    // and clean up
    s.client
        .delete(&s.blank, added.kind, &added.id)
        .await
        .unwrap();
    s.client
        .delete(&s.blank, EntityKind::Folder, &folder.id)
        .await
        .unwrap();
    let tree = s.client.tree(&s.blank).await.unwrap();
    assert!(tree.find("canopy-probe").is_none());
}

#[tokio::test]
#[ignore = "needs the local Overleaf CE"]
async fn a_bad_session_reads_as_signed_out() {
    let server = Server::parse("http://localhost:8090").unwrap();
    let client = Client::new(server, "overleaf.sid=s%3Anot-a-real-session").unwrap();
    assert_eq!(
        client.projects().await.unwrap_err(),
        canopy_overleaf::Error::SignedOut
    );
}
