use super::*;
use crate::edit::model::{Colour, CropRect, IDENTITY};
use crate::media::format_of;

fn picture(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Pixels {
    let mut rgba = Vec::new();
    for y in 0..h {
        for x in 0..w {
            rgba.extend(f(x, y));
        }
    }
    Pixels {
        width: w,
        height: h,
        full_width: w,
        full_height: h,
        rgba,
        has_alpha: false,
    }
}

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

#[test]
fn a_flip_puts_red_on_the_right() {
    let src = picture(4, 2, |x, _| if x < 2 { RED } else { BLUE });
    let (_, out) = render_pixels(
        &src,
        &PhotoEdit {
            flip: true,
            ..IDENTITY
        },
    );
    assert_eq!(out.pixel(3, 0), RED);
    assert_eq!(out.pixel(0, 1), BLUE);
}

#[test]
fn the_marked_corner_follows_each_quarter_turn() {
    // The top-left pixel is marked.
    let src = picture(4, 2, |x, y| if (x, y) == (0, 0) { RED } else { WHITE });
    for (rotate, size, at) in [
        (0, (4, 2), (0, 0)),
        (90, (2, 4), (1, 0)),
        (180, (4, 2), (3, 1)),
        (270, (2, 4), (0, 3)),
    ] {
        let (_, out) = render_pixels(&src, &PhotoEdit { rotate, ..IDENTITY });
        assert_eq!((out.width, out.height), size, "{rotate}");
        assert_eq!(out.pixel(at.0, at.1), RED, "{rotate}");
    }
}

#[test]
fn a_quadrant_crop_is_one_colour() {
    let src = picture(400, 200, |x, y| match (x < 200, y < 100) {
        (true, true) => RED,
        (false, true) => GREEN,
        (true, false) => BLUE,
        (false, false) => WHITE,
    });
    let e = PhotoEdit {
        crop: CropRect {
            x: 0.5,
            y: 0.0,
            w: 0.5,
            h: 0.5,
        },
        ..IDENTITY
    };
    let (_, out) = render_pixels(&src, &e);
    assert_eq!((out.width, out.height), (200, 100));
    assert!(out.rgba.as_chunks::<4>().0.iter().all(|p| *p == GREEN));
}

#[test]
fn a_turned_crop_keeps_the_same_part_of_the_picture() {
    // The top-left quadrant is red; crop it, then turn and flip the edit.
    let src = picture(40, 20, |x, y| if x < 20 && y < 10 { RED } else { WHITE });
    let e = PhotoEdit {
        crop: CropRect {
            x: 0.0,
            y: 0.0,
            w: 0.5,
            h: 0.5,
        },
        ..IDENTITY
    };
    for op in [
        crate::edit::model::rotate_left,
        crate::edit::model::rotate_right,
        crate::edit::model::flip,
    ] {
        let (_, out) = render_pixels(&src, &op(&e));
        assert!(out.rgba.as_chunks::<4>().0.iter().all(|p| *p == RED));
    }
}

#[test]
fn straightening_never_samples_outside_the_picture() {
    // A solid red picture: any empty corner would show as black.
    let src = picture(60, 40, |_, _| RED);
    let (plan, out) = render_pixels(
        &src,
        &PhotoEdit {
            straighten: 10.0,
            ..IDENTITY
        },
    );
    assert!(plan.frame.w < 60 && plan.frame.h < 40);
    assert!(out.rgba.as_chunks::<4>().0.iter().all(|p| *p == RED));
}

#[test]
fn colour_on_pixels() {
    let grey = picture(8, 8, |_, _| [128, 128, 128, 255]);
    let e = PhotoEdit {
        colour: Colour {
            brightness: 50.0,
            ..Colour::NONE
        },
        ..IDENTITY
    };
    let (_, out) = render_pixels(&grey, &e);
    assert_eq!(out.pixel(0, 0), [179, 179, 179, 255], "+51 levels");
    let red = picture(2, 2, |_, _| RED);
    let e = PhotoEdit {
        colour: Colour {
            saturation: -100.0,
            ..Colour::NONE
        },
        ..IDENTITY
    };
    let p = render_pixels(&red, &e).1.pixel(0, 0);
    assert!(p[0] == p[1] && p[1] == p[2], "{p:?}");
}

#[test]
fn render_edit_writes_a_file_through_a_temporary() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("in.png");
    let px = picture(40, 20, |x, _| if x < 20 { RED } else { BLUE });
    fs::write(&src, encode(&px, SaveFormat::Png).unwrap()).unwrap();
    let out = tmp.path().join("out.jpg");
    let format = format_of("in.png").unwrap();
    let e = PhotoEdit {
        rotate: 90,
        ..IDENTITY
    };
    assert_eq!(
        render_edit(&src, format, &e, SaveFormat::Jpeg, &out).unwrap(),
        (20, 40)
    );
    let back = image::open(&out).unwrap().to_rgb8();
    assert_eq!(back.dimensions(), (20, 40));
    let files: Vec<_> = fs::read_dir(tmp.path()).unwrap().collect();
    assert_eq!(files.len(), 2, "no temporary left behind");
    let png_out = tmp.path().join("out.png");
    render_edit(&src, format, &e, SaveFormat::Png, &png_out).unwrap();
    assert_eq!(
        image::open(&png_out).unwrap().to_rgb8().get_pixel(0, 0).0,
        [255, 0, 0]
    );
}

#[test]
fn transparency_is_flattened_onto_white_for_jpeg() {
    let mut px = picture(8, 8, |_, _| [0, 0, 0, 0]);
    px.has_alpha = true;
    let jpeg = encode(&px, SaveFormat::Jpeg).unwrap();
    let back = image::load_from_memory(&jpeg).unwrap().to_rgb8();
    assert!(back.pixels().all(|p| p.0.iter().all(|v| *v > 250)));
}

// ---- The file's own depth, profile and metadata (#150) ----

mod file {
    use owlbum_fixtures::encode::{self, Meta};
    use owlbum_fixtures::icc;
    use owlbum_fixtures::picture;

    use super::*;
    use crate::edit::metadata::{exif_of, has_thumbnail, xmp_of};
    use crate::media::exif::{Dir, Exif};

    fn crop_half() -> PhotoEdit {
        PhotoEdit {
            crop: CropRect {
                x: 0.0,
                y: 0.0,
                w: 0.5,
                h: 0.5,
            },
            ..IDENTITY
        }
    }

    fn brighter() -> PhotoEdit {
        PhotoEdit {
            colour: Colour {
                brightness: 20.0,
                ..Colour::NONE
            },
            ..IDENTITY
        }
    }

    fn render(
        name: &str,
        bytes: &[u8],
        e: &PhotoEdit,
        target: SaveFormat,
    ) -> (Vec<u8>, (u32, u32)) {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join(name);
        fs::write(&src, bytes).unwrap();
        let out = tmp.path().join("out");
        let size = render_edit(&src, format_of(name).unwrap(), e, target, &out).unwrap();
        assert_eq!(fs::read(&src).unwrap(), bytes, "the source is unchanged");
        (fs::read(out).unwrap(), size)
    }

    fn camera(orientation: u16) -> Meta {
        Meta {
            make: Some("Owlcam"),
            model: Some("Hoot 7"),
            lens: Some("Owlcam 35mm"),
            orientation,
            taken: Some("2024:06:01 10:20:30".into()),
            exposure: Some([1, 250]),
            iso: Some(200),
            gps: Some((48_858_000, 2_294_000)),
            pixels: Some((64, 32)),
            ..Meta::default()
        }
    }

    #[test]
    fn an_exif_rotated_photo_comes_out_upright_with_orientation_1_and_its_size() {
        // Stored turned (orientation 6): upright it is 32 wide and 64 high,
        // with its red quadrant at the top left.
        let upright = picture::quadrants(32, 64);
        let stored = picture::store_for_orientation(&upright, 6);
        let jpeg = encode::jpeg(&stored, 95, &camera(6));
        let (out, size) = render("trip.jpg", &jpeg, &crop_half(), SaveFormat::Jpeg);
        assert_eq!(size, (16, 32));
        let back = image::load_from_memory(&out).unwrap().to_rgb8();
        assert_eq!(back.dimensions(), (16, 32));
        let want = upright.get_pixel(4, 4).0;
        let got = back.get_pixel(4, 4).0;
        assert!(
            want.iter().zip(got).all(|(a, b)| a.abs_diff(b) <= 12),
            "{want:?} {got:?}"
        );
        let block = exif_of(&out).expect("EXIF kept");
        let tags = Exif::parse(&block).unwrap();
        assert_eq!(tags.int(Dir::Main, 0x0112), Some(1));
        assert_eq!(tags.int(Dir::Exif, 0xa002), Some(16));
        assert_eq!(tags.int(Dir::Exif, 0xa003), Some(32));
        assert!(!has_thumbnail(&block));
    }

    #[test]
    fn camera_lens_date_and_gps_tags_are_equal_before_and_after() {
        let jpeg = encode::jpeg(&picture::quadrants(64, 32), 95, &camera(1));
        let before = Exif::parse(&exif_of(&jpeg).unwrap()).unwrap();
        for target in [SaveFormat::Jpeg, SaveFormat::Png, SaveFormat::Webp] {
            let (out, _) = render("a.jpg", &jpeg, &brighter(), target);
            let after = Exif::parse(&exif_of(&out).expect("EXIF kept")).unwrap();
            for (dir, tag) in [
                (Dir::Main, 0x010f),
                (Dir::Main, 0x0110),
                (Dir::Exif, 0xa434),
                (Dir::Exif, 0x9003),
                (Dir::Exif, 0x829a),
                (Dir::Gps, 0x0002),
                (Dir::Gps, 0x0004),
            ] {
                assert_eq!(
                    before.get(dir, tag),
                    after.get(dir, tag),
                    "{target:?} {tag:#06x}"
                );
            }
            assert_eq!(
                after.int(Dir::Exif, 0xa001),
                Some(1),
                "a colour edit tags sRGB"
            );
        }
    }

    #[test]
    fn xmp_is_kept_as_it_is() {
        let png = encode::png_rgb(&picture::quadrants(8, 8));
        let xmp = b"<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF/></x:xmpmeta>";
        let png = crate::edit::metadata::png_with_xmp(png, xmp);
        for target in [SaveFormat::Jpeg, SaveFormat::Png, SaveFormat::Webp] {
            let (out, _) = render("a.png", &png, &crop_half(), target);
            assert_eq!(xmp_of(&out).as_deref(), Some(&xmp[..]), "{target:?}");
        }
    }

    fn icc_of(bytes: &[u8]) -> Option<Vec<u8>> {
        use image::ImageDecoder;
        let mut d = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .unwrap()
            .into_decoder()
            .unwrap();
        d.icc_profile().unwrap()
    }

    #[test]
    fn a_wide_gamut_profile_is_kept_on_a_crop_and_becomes_srgb_on_a_colour_edit() {
        let p3 = icc::DISPLAY_P3.bytes();
        let png = encode::png_with_icc(&picture::quadrants(16, 16), &p3);
        for target in [SaveFormat::Png, SaveFormat::Jpeg, SaveFormat::Webp] {
            let (out, _) = render("wide.png", &png, &crop_half(), target);
            assert_eq!(
                icc_of(&out).as_deref(),
                Some(&p3[..]),
                "{target:?}: profile bytes unchanged"
            );
            // A crop leaves the pixels in the profile, not converted.
            let px = image::load_from_memory(&out).unwrap().to_rgb8();
            let p = px.get_pixel(2, 2).0;
            assert!(p[0] > 245 && p[1] < 10 && p[2] < 10, "{target:?} {p:?}");
        }
        let (out, _) = render("wide.png", &png, &brighter(), SaveFormat::Png);
        let srgb = icc_of(&out).expect("tagged");
        assert!(crate::media::colour::is_srgb(Some(&srgb)), "tagged sRGB");
        assert_ne!(srgb, p3);
    }

    #[test]
    fn a_16_bit_png_stays_16_bit() {
        let png = encode::png_rgb16(&picture::quadrants(16, 16));
        let (out, size) = render("deep.png", &png, &crop_half(), SaveFormat::Png);
        assert_eq!(size, (8, 8));
        let img = image::load_from_memory(&out).unwrap();
        assert_eq!(img.color(), image::ColorType::Rgb16);
        // The low-order detail survives: the samples match the source's.
        let src = image::load_from_memory(&png).unwrap().to_rgb16();
        assert_eq!(img.to_rgb16().get_pixel(3, 5), src.get_pixel(3, 5));
        // As JPEG it is 8-bit.
        let (jpeg, _) = render("deep.png", &png, &crop_half(), SaveFormat::Jpeg);
        assert_eq!(
            image::load_from_memory(&jpeg).unwrap().color(),
            image::ColorType::Rgb8
        );
    }

    #[test]
    fn a_small_dng_saved_as_jpeg_carries_its_exif() {
        let meta = Meta {
            make: Some("Owlbum"),
            model: Some("Test Camera"),
            taken: Some("2024:06:12 14:03:00".into()),
            ..Meta::default()
        };
        let dng = owlbum_fixtures::dng::dng(&picture::quadrants(32, 16), &meta);
        let (out, size) = render("IMG_1.dng", &dng, &IDENTITY, SaveFormat::Jpeg);
        assert_eq!(image::load_from_memory(&out).unwrap().width(), size.0);
        let tags = Exif::parse(&exif_of(&out).expect("EXIF from the DNG")).unwrap();
        assert_eq!(tags.text(Dir::Main, 0x010f), Some("Owlbum"));
        assert_eq!(tags.text(Dir::Exif, 0x9003), Some("2024:06:12 14:03:00"));
        assert_eq!(tags.int(Dir::Main, 0x0112), Some(1));
        assert_eq!(tags.get(Dir::Main, 0x0111), None, "no strip offsets");
    }
}
