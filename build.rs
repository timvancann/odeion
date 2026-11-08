// On Windows, embeds Odeion's icon (bundle/odeion.rc) in odeion.exe. GPUI embeds its own
// manifest the same way; an icon and a manifest don't collide. bundle/icon.ico comes from
// bundle/icon.svg: resvg at 16 to 256 px, joined with ImageMagick's `magick`.
fn main() {
    println!("cargo:rerun-if-changed=bundle/odeion.rc");
    println!("cargo:rerun-if-changed=bundle/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("bundle/odeion.rc", embed_resource::NONE).manifest_optional().unwrap();
    }
}
