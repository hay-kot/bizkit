//! Re-embeds the web app when `web/dist` changes. Without this, a release
//! build made before `web/dist` existed would never pick it up, because
//! `allow_missing` lets the derive succeed against an empty folder.

fn main() {
    println!("cargo::rerun-if-changed=../../web/dist");
}
