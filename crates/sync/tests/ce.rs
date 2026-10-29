//! A full sync against the local Overleaf CE in dev/overleaf-ce:
//!
//!     docker compose -f dev/overleaf-ce/compose.yml up -d
//!     python dev/overleaf-ce/seed.py
//!     cargo test -p canopy-sync --test ce -- --ignored
//!
//! Works on the seeded "Canopy blank test" project and puts it back the way
//! it found it.

use canopy_overleaf::{Client, Server};
use canopy_sync::{sync, Options, OverleafRemote, Remote, Store};

fn remote() -> OverleafRemote {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../dev/overleaf-ce/.test-user.json"
    );
    let state: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(path).expect("run dev/overleaf-ce/seed.py first"),
    )
    .unwrap();
    let client = Client::new(
        Server::parse("http://localhost:8090").unwrap(),
        state["cookie"].as_str().unwrap(),
    )
    .unwrap();
    OverleafRemote::new(client, state["projects"]["blank"].as_str().unwrap())
}

#[tokio::test]
#[ignore = "needs the local Overleaf CE"]
async fn two_way_sync_with_a_real_server() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("blank");
    std::fs::create_dir(&folder).unwrap();
    let store = dir.path().join("history.git");
    let remote = remote();
    let opts = Options::default();

    // first sync: everything comes down
    let r = sync(&remote, &folder, &store, opts).await.unwrap();
    assert!(r.downloaded.iter().any(|p| p == "main.tex"), "{r:?}");
    let original_main = std::fs::read(folder.join("main.tex")).unwrap();

    // local edits and new files go up, folders and all
    std::fs::write(
        folder.join("main.tex"),
        "\\documentclass{article}\n\\begin{document}\nFrom Canopy.\n\\end{document}\n",
    )
    .unwrap();
    std::fs::create_dir_all(folder.join("canopy-e2e")).unwrap();
    std::fs::write(folder.join("canopy-e2e").join("notes.tex"), "notes\n").unwrap();
    std::fs::write(
        folder.join("canopy-e2e").join("dot.png"),
        [0x89, b'P', b'N', b'G', 0, 1, 2],
    )
    .unwrap();
    let r = sync(&remote, &folder, &store, opts).await.unwrap();
    let mut up = r.uploaded.clone();
    up.sort();
    assert_eq!(
        up,
        vec!["canopy-e2e/dot.png", "canopy-e2e/notes.tex", "main.tex"]
    );

    // the server really has them
    let server = remote.fetch().await.unwrap().files;
    assert_eq!(server["canopy-e2e/notes.tex"], b"notes\n");
    assert!(String::from_utf8_lossy(&server["main.tex"]).contains("From Canopy."));

    // a second sync has nothing to do
    let r = sync(&remote, &folder, &store, opts).await.unwrap();
    assert!(r.is_quiet(), "{r:?}");

    // deleting locally deletes on the server
    std::fs::remove_dir_all(folder.join("canopy-e2e")).unwrap();
    let r = sync(&remote, &folder, &store, opts).await.unwrap();
    assert_eq!(r.deleted_remote.len(), 2, "{r:?}");
    assert!(!remote
        .fetch()
        .await
        .unwrap()
        .files
        .contains_key("canopy-e2e/notes.tex"));

    // put main.tex back
    std::fs::write(folder.join("main.tex"), &original_main).unwrap();
    sync(&remote, &folder, &store, opts).await.unwrap();

    assert!(Store::open(&store).unwrap().history(20).unwrap().len() >= 4);
}
