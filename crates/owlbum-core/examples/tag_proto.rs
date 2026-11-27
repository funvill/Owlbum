//! The tagging prototype (docs/07-tagging.md, "Prototype plan"; #52).
//!
//! `cargo run -p owlbum-core --release --example tag_proto -- <models dir>
//! [picture ...]` with `OWLBUM_ORT_DYLIB` naming ONNX Runtime's library.
//! Loads `florence2-base-q8` and `clip-base-q8` from `<models dir>/<id>/`,
//! describes and keywords the fixture lake and harbour pictures (and any
//! pictures given), and prints what came out and how long it took. It never
//! downloads anything.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use owlbum_core::tagging::backend::{Deadline, TagBackend, TagImage};
use owlbum_core::tagging::catalogue::Catalogue;
use owlbum_core::tagging::onnx::{self, OnnxBackend, Pick};
use owlbum_core::tagging::results::dot;
use owlbum_core::tagging::vocabulary::VOCABULARY;

fn main() {
    let mut args = std::env::args().skip(1);
    let models = PathBuf::from(args.next().expect("models folder"));
    let mut pictures: Vec<(String, image::RgbImage)> = vec![
        ("lake".into(), owlbum_fixtures::picture::lake(640, 480)),
        ("harbour".into(), owlbum_fixtures::picture::harbour(640, 480)),
    ];
    for p in args {
        let img = image::open(&p).expect("picture").to_rgb8();
        pictures.push((p, img));
    }
    let threads = onnx::default_threads();
    let mut b = OnnxBackend::new().expect("runtime");
    let cat = Catalogue::real();
    let long = || Deadline::after(Duration::from_secs(600));
    for id in ["clip-base-q8", "florence2-base-q8"] {
        if !models.join(id).is_dir() {
            println!("{id}: not in {}", models.display());
            continue;
        }
        let t = Instant::now();
        b.load(cat.get(id).unwrap(), &models.join(id), threads, VOCABULARY, &long())
            .expect("load");
        println!("{id}: loaded in {:.2} s with {threads} threads", t.elapsed().as_secs_f32());
    }
    let text = b.text_embedder();
    let seaside = text.as_ref().map(|t| t.embed("seaside", &long()).unwrap());
    for (name, rgb) in &pictures {
        let img = TagImage { rgb, name };
        println!("== {name}");
        if b.loaded(owlbum_core::tagging::catalogue::Part::Keywords).is_some() {
            let t = Instant::now();
            let k = b.keywords(&img, &long()).expect("keywords");
            println!("  keywords in {:.3} s, dim {}", t.elapsed().as_secs_f32(), k.vector.len());
            for kw in &k.keywords {
                println!("    {} {:.3}", kw.label, kw.score);
            }
            if let Some(s) = &seaside {
                println!("  cosine with \"seaside\": {:.3}", dot(s, &k.vector));
            }
        }
        if b.loaded(owlbum_core::tagging::catalogue::Part::Describe).is_some() {
            let t = Instant::now();
            let d = b.describe(&img, &long()).expect("describe");
            println!("  describe in {:.2} s", t.elapsed().as_secs_f32());
            println!("    {:?}", d.description);
            println!("    {:?}", d.objects);
            for _ in 0..3 {
                let t = Instant::now();
                let c = b.caption_with(&img, onnx::SAMPLED, &long()).expect("caption");
                println!("  sampled in {:.2} s: {c}", t.elapsed().as_secs_f32());
            }
            let t = Instant::now();
            let r = b.caption_with(&img, Pick::Greedy, &Deadline::after(Duration::from_millis(500)));
            println!("  a 0.5 s deadline: {r:?} after {:.2} s", t.elapsed().as_secs_f32());
        }
    }
}
