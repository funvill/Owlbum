//! The shared plan fixtures (docs/06-editing-and-file-actions.md, "The test
//! that pins agreement", test 1).
//!
//! `fixtures/edit-plans.json` holds edits, photo sizes and boxes, each with
//! its plan and what `EditedImage` must set. This test fails when the
//! model's output differs from the committed file; the page test
//! (`ui/src/features/edit/EditedImage.test.tsx`) reads the same file. To
//! write the file again after a deliberate change, run this test with
//! `OWLBUM_UPDATE_FIXTURES=1`.

use std::path::PathBuf;

use serde_json::{Value, json};

use super::model::{Colour, CropRect, IDENTITY, PhotoEdit};
use super::plan::{Fit, draw, edit_plan};

fn path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/edit-plans.json")
}

fn cases() -> Vec<(&'static str, PhotoEdit, (u32, u32))> {
    let crop = CropRect {
        x: 0.1,
        y: 0.05,
        w: 0.8,
        h: 0.9,
    };
    vec![
        ("identity", IDENTITY, (3000, 2000)),
        (
            "rotate 90",
            PhotoEdit {
                rotate: 90,
                ..IDENTITY
            },
            (3000, 2000),
        ),
        (
            "rotate 180",
            PhotoEdit {
                rotate: 180,
                ..IDENTITY
            },
            (3000, 2000),
        ),
        (
            "rotate 270",
            PhotoEdit {
                rotate: 270,
                ..IDENTITY
            },
            (3000, 2000),
        ),
        (
            "flip",
            PhotoEdit {
                flip: true,
                ..IDENTITY
            },
            (3000, 2000),
        ),
        (
            "flip and rotate 90",
            PhotoEdit {
                rotate: 90,
                flip: true,
                ..IDENTITY
            },
            (4032, 3024),
        ),
        (
            "straighten",
            PhotoEdit {
                straighten: -2.5,
                ..IDENTITY
            },
            (3000, 2000),
        ),
        ("crop", PhotoEdit { crop, ..IDENTITY }, (1600, 1200)),
        (
            "everything",
            PhotoEdit {
                rotate: 90,
                flip: false,
                straighten: -2.5,
                crop,
                colour: Colour {
                    brightness: 10.0,
                    contrast: 0.0,
                    saturation: -20.0,
                    warmth: 5.0,
                },
            },
            (3000, 2000),
        ),
        (
            "colour only",
            PhotoEdit {
                colour: Colour {
                    brightness: 0.0,
                    contrast: 35.0,
                    saturation: 40.0,
                    warmth: -30.0,
                },
                ..IDENTITY
            },
            (512, 341),
        ),
        (
            "tiny crop",
            PhotoEdit {
                crop: CropRect {
                    x: 0.99,
                    y: 0.0,
                    w: 0.01,
                    h: 0.01,
                },
                ..IDENTITY
            },
            (120, 80),
        ),
    ]
}

fn build() -> Value {
    let boxes = [(200.0, 200.0), (640.0, 360.0)];
    let mut out = Vec::new();
    for (name, edit, (w, h)) in cases() {
        let plan = edit_plan(w, h, &edit, false);
        for (bw, bh) in boxes {
            for (fit, fit_name) in [(Fit::Contain, "contain"), (Fit::Cover, "cover")] {
                let d = draw(&plan, bw, bh, fit);
                out.push(json!({
                    "name": name,
                    "edit": edit,
                    "photo": { "w": w, "h": h },
                    "box": { "w": bw, "h": bh },
                    "fit": fit_name,
                    "plan": plan,
                    "expect": {
                        "outer_w": d.outer_w,
                        "outer_h": d.outer_h,
                        "width": d.img_w,
                        "height": d.img_h,
                        "transform": d.transform,
                        "filter": d.filter,
                    },
                }));
            }
        }
    }
    Value::Array(out)
}

#[test]
fn the_shared_plan_fixtures_match_the_model() {
    let want = build();
    let text = serde_json::to_string_pretty(&want).unwrap() + "\n";
    if std::env::var_os("OWLBUM_UPDATE_FIXTURES").is_some() {
        std::fs::create_dir_all(path().parent().unwrap()).unwrap();
        std::fs::write(path(), &text).unwrap();
        return;
    }
    // Compared as text: serde_json does not read every float back exactly.
    let have = std::fs::read_to_string(path())
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        have == text,
        "fixtures/edit-plans.json differs from the model; run this test with OWLBUM_UPDATE_FIXTURES=1 after a deliberate change"
    );
}
