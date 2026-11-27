use owlbum_fixtures::encode::{self, Meta};
use owlbum_fixtures::tiff::{self, Ifd, Val};

use super::*;
use crate::media::exif::{Dir, Exif as Tags};

fn camera() -> Meta {
    Meta {
        make: Some("Owlcam"),
        model: Some("Hoot 7"),
        lens: Some("Owlcam 35mm"),
        orientation: 6,
        taken: Some("2024:06:01 10:20:30".into()),
        offset: Some("+02:00"),
        exposure: Some([1, 250]),
        f_number_x10: Some(28),
        iso: Some(200),
        focal_mm: Some(35),
        gps: Some((48_858_000, 2_294_000)),
        pixels: Some((60, 40)),
        ..Meta::default()
    }
}

/// `block` with an IFD 1 holding a fake JPEG thumbnail at its end, as
/// cameras write it.
fn with_thumbnail(mut block: Vec<u8>) -> Vec<u8> {
    let at = u32::from_le_bytes(block[4..8].try_into().unwrap()) as usize;
    let n = usize::from(u16::from_le_bytes([block[at], block[at + 1]]));
    let next_at = at + 2 + n * 12;
    if block.len() % 2 == 1 {
        block.push(0);
    }
    let ifd1 = block.len() as u32;
    block[next_at..next_at + 4].copy_from_slice(&ifd1.to_le_bytes());
    let thumb = [0xff, 0xd8, 0xff, 0xd9, 1, 2, 3, 4];
    let thumb_at = ifd1 + 2 + 2 * 12 + 4;
    block.extend(2u16.to_le_bytes());
    for (tag, v) in [(0x0201u16, thumb_at), (0x0202, thumb.len() as u32)] {
        block.extend(tag.to_le_bytes());
        block.extend(4u16.to_le_bytes());
        block.extend(1u32.to_le_bytes());
        block.extend(v.to_le_bytes());
    }
    block.extend(0u32.to_le_bytes());
    block.extend(thumb);
    block
}

const CHANGED: Changes = Changes {
    width: 30,
    height: 20,
    srgb: false,
};

fn same_tags(a: &Tags, b: &Tags) {
    for (dir, tag) in [
        (Dir::Main, 0x010f),
        (Dir::Main, 0x0110),
        (Dir::Main, 0x0132),
        (Dir::Exif, 0x9003),
        (Dir::Exif, 0x829a),
        (Dir::Exif, 0x829d),
        (Dir::Exif, 0x8827),
        (Dir::Exif, 0xa434),
        (Dir::Gps, 0x0001),
        (Dir::Gps, 0x0002),
        (Dir::Gps, 0x0003),
        (Dir::Gps, 0x0004),
    ] {
        assert!(a.get(dir, tag).is_some(), "{tag:#06x} is in the original");
        assert_eq!(a.get(dir, tag), b.get(dir, tag), "{tag:#06x}");
    }
}

#[test]
fn a_jpeg_block_keeps_its_tags_with_orientation_1_the_new_size_and_no_thumbnail() {
    let block = with_thumbnail(camera().exif_block().unwrap());
    assert!(has_thumbnail(&block));
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("a.jpg");
    let img = owlbum_fixtures::picture::quadrants(8, 8);
    let jpeg = encode::jpeg(&img, 90, &Meta::default());
    // Put the block (with its thumbnail) in by hand.
    let mut bytes = jpeg[..2].to_vec();
    bytes.extend([0xff, 0xe1]);
    bytes.extend(u16::try_from(block.len() + 8).unwrap().to_be_bytes());
    bytes.extend(b"Exif\0\0");
    bytes.extend(&block);
    bytes.extend(&jpeg[2..]);
    fs::write(&file, &bytes).unwrap();

    let kept = read(&file);
    let out = kept.exif_for(CHANGED, JPEG_EXIF_MAX).unwrap();
    let before = Tags::parse(&block).unwrap();
    let after = Tags::parse(&out).unwrap();
    assert_eq!(after.int(Dir::Main, 0x0112), Some(1));
    assert_eq!(after.int(Dir::Exif, 0xa002), Some(30));
    assert_eq!(after.int(Dir::Exif, 0xa003), Some(20));
    assert_eq!(after.get(Dir::Exif, 0xa001), None, "ColorSpace untouched");
    same_tags(&before, &after);
    assert!(!has_thumbnail(&out));
    assert!(
        !out.windows(4).any(|w| w == [0xff, 0xd8, 0xff, 0xd9]),
        "thumbnail bytes are gone"
    );
}

#[test]
fn a_colour_change_sets_colour_space_to_srgb() {
    let block = camera().exif_block().unwrap();
    let kept = Kept {
        exif: Some(Exif::Block(block)),
        xmp: None,
    };
    let out = kept
        .exif_for(
            Changes {
                srgb: true,
                ..CHANGED
            },
            usize::MAX,
        )
        .unwrap();
    assert_eq!(Tags::parse(&out).unwrap().int(Dir::Exif, 0xa001), Some(1));
}

#[test]
fn a_block_without_an_exif_ifd_gets_one_for_the_size() {
    let mut ifd0 = Ifd::new();
    ifd0.ascii(0x010f, "Owlcam").short(0x0112, 8);
    let kept = Kept {
        exif: Some(Exif::Block(tiff::write(&ifd0))),
        xmp: None,
    };
    let after = Tags::parse(&kept.exif_for(CHANGED, usize::MAX).unwrap()).unwrap();
    assert_eq!(after.text(Dir::Main, 0x010f), Some("Owlcam"));
    assert_eq!(after.int(Dir::Main, 0x0112), Some(1));
    assert_eq!(after.int(Dir::Exif, 0xa002), Some(30));
}

#[test]
fn a_tiff_file_gives_a_compact_block_without_its_pixel_tags_or_maker_note() {
    let mut exif = Ifd::new();
    exif.ascii(0x9003, "2024:06:01 10:20:30")
        .set(0x927c, Val::Undefined(vec![7; 64]))
        .short(0x8827, 400);
    let mut ifd0 = Ifd::new();
    ifd0.long(0x0100, 4)
        .long(0x0101, 2)
        .short(0x0103, 1)
        .set(0x0111, Val::Blob(vec![9; 24]))
        .long(0x0117, 24)
        .ascii(0x010f, "Owlcam")
        .short(0x0112, 3)
        .set(0x02bc, Val::Undefined(b"<x:xmpmeta/>".to_vec()))
        .set(0x8769, Val::Ifds(vec![exif]));
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("a.tif");
    fs::write(&file, tiff::write(&ifd0)).unwrap();

    let kept = read(&file);
    assert_eq!(kept.xmp.as_deref(), Some(&b"<x:xmpmeta/>"[..]));
    let out = kept.exif_for(CHANGED, JPEG_EXIF_MAX).unwrap();
    let after = Tags::parse(&out).unwrap();
    assert_eq!(after.text(Dir::Main, 0x010f), Some("Owlcam"));
    assert_eq!(after.int(Dir::Main, 0x0112), Some(1));
    assert_eq!(after.get(Dir::Main, 0x0100), None);
    assert_eq!(after.get(Dir::Main, 0x0111), None);
    assert_eq!(after.text(Dir::Exif, 0x9003), Some("2024:06:01 10:20:30"));
    assert_eq!(after.int(Dir::Exif, 0x8827), Some(400));
    assert_eq!(after.get(Dir::Exif, 0x927c), None);
    assert_eq!(after.int(Dir::Exif, 0xa003), Some(20));
}

#[test]
fn camera_tags_survive_the_compact_form_too() {
    let block = camera().exif_block().unwrap();
    let Some(Kept {
        exif: Some(Exif::Compact(t)),
        ..
    }) = compact_from(&mut Cursor::new(&block))
    else {
        panic!("compact");
    };
    let after = Tags::parse(&t.write(CHANGED).unwrap()).unwrap();
    same_tags(&Tags::parse(&block).unwrap(), &after);
}

#[test]
fn xmp_goes_into_a_png_and_reads_back() {
    let png = encode::png_rgb(&owlbum_fixtures::picture::quadrants(4, 4));
    let xmp = b"<x:xmpmeta xmlns:x='adobe:ns:meta/'/>";
    let out = png_with_xmp(png, xmp);
    assert_eq!(xmp_of(&out).as_deref(), Some(&xmp[..]));
    // The chunk is valid: the PNG decoder checks CRCs.
    image::load_from_memory(&out).unwrap();
}

#[test]
fn files_without_metadata_keep_nothing_and_damage_never_panics() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("a.png");
    fs::write(
        &file,
        encode::png_rgb(&owlbum_fixtures::picture::quadrants(4, 4)),
    )
    .unwrap();
    assert!(read(&file).is_empty());
    let block = camera().exif_block().unwrap();
    for cut in [0, 7, 9, 20, block.len() / 2] {
        let kept = Kept {
            exif: Some(Exif::Block(block[..cut].to_vec())),
            xmp: None,
        };
        let _ = kept.exif_for(CHANGED, usize::MAX);
    }
    for name in ["b.jpg", "c.tif"] {
        let f = tmp.path().join(name);
        fs::write(
            &f,
            if name.ends_with("jpg") {
                &b"\xff\xd8\xff\xe1\xff\xff"[..]
            } else {
                &b"II*\0\xff\xff\xff\xff"[..]
            },
        )
        .unwrap();
        assert!(read(&f).exif.is_none());
    }
}
