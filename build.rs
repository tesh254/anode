//! Rasterize the standalone Anode mark for runtime and macOS bundle icons.

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=assets/app-icon.svg");

    let svg = std::fs::read("assets/app-icon.svg").expect("read assets/app-icon.svg");
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(&svg, &opt).expect("parse assets/app-icon.svg");

    // The source includes the rounded tile and its safe-area inset.
    const CANVAS: u32 = 1024;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(CANVAS, CANVAS).expect("allocate icon pixmap");
    let size = tree.size();
    let scale = CANVAS as f32 / size.width().max(size.height());
    let x = (CANVAS as f32 - size.width() * scale) / 2.0;
    let y = (CANVAS as f32 - size.height() * scale) / 2.0;
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale).post_translate(x, y);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let png = pixmap.encode_png().expect("encode icon PNG");
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let out_path = Path::new(&out_dir).join("app-icon.png");
    std::fs::write(out_path, png).expect("write app-icon.png");
}
