//! A small DNG laid out like a camera's or Adobe's converter's: IFD 0 is an
//! uncompressed RGB thumbnail, and two SubIFDs hold the raw Bayer data
//! (16-bit, RGGB, 12 bits used) and a full-size JPEG preview.

use image::RgbImage;

use crate::encode::{self, Meta};
use crate::picture::shrink;
use crate::tiff::{self, Ifd, Val};

const WHITE: u16 = 4095;

fn strip(ifd: &mut Ifd, w: u32, h: u32, data: Vec<u8>) {
    ifd.long(256, w).long(257, h);
    ifd.long(279, u32::try_from(data.len()).expect("strip"));
    ifd.set(273, Val::Blob(data));
    ifd.long(278, h);
    ifd.short(284, 1);
}

/// A DNG of `picture` (its size is the raw size; use even numbers).
pub fn dng(picture: &RgbImage, meta: &Meta) -> Vec<u8> {
    let (w, h) = picture.dimensions();

    // The raw image: one sample per pixel through an RGGB filter.
    let mut raw = Vec::with_capacity((w * h * 2) as usize);
    for y in 0..h {
        for x in 0..w {
            let p = picture.get_pixel(x, y).0;
            let channel = match (y % 2, x % 2) {
                (0, 0) => 0,
                (1, 1) => 2,
                _ => 1,
            };
            let v = (u32::from(p[channel]) * u32::from(WHITE) / 255) as u16;
            raw.extend_from_slice(&v.to_le_bytes());
        }
    }
    let mut raw_ifd = Ifd::new();
    raw_ifd.long(254, 0);
    strip(&mut raw_ifd, w, h, raw);
    raw_ifd
        .short(258, 16)
        .short(259, 1)
        .short(262, 32803)
        .short(277, 1);
    raw_ifd.set(33421, Val::Short(vec![2, 2]));
    raw_ifd.set(33422, Val::Byte(vec![0, 1, 1, 2]));
    raw_ifd.short(50714, 0).short(50717, WHITE);
    raw_ifd.set(50719, Val::Long(vec![0, 0])); // DefaultCropOrigin
    raw_ifd.set(50720, Val::Long(vec![w, h])); // DefaultCropSize

    // The embedded preview, as cameras store it.
    let preview_jpeg = encode::jpeg(picture, 90, &Meta::default());
    let mut preview = Ifd::new();
    preview.long(254, 1);
    strip(&mut preview, w, h, preview_jpeg);
    preview.set(258, Val::Short(vec![8, 8, 8]));
    preview.short(259, 7).short(262, 6).short(277, 3);

    // IFD 0: a small uncompressed thumbnail plus the DNG and camera tags.
    let thumb = shrink(picture, 4);
    let (tw, th) = thumb.dimensions();
    let mut ifd0 = Ifd::new();
    ifd0.long(254, 1);
    strip(&mut ifd0, tw, th, thumb.into_raw());
    ifd0.set(258, Val::Short(vec![8, 8, 8]));
    ifd0.short(259, 1).short(262, 2).short(277, 3).short(274, 1);
    ifd0.ascii(305, "owlbum-fixtures");
    if let Some(m) = meta.make {
        ifd0.ascii(271, m);
    }
    if let Some(m) = meta.model {
        ifd0.ascii(272, m);
    }
    if let Some(t) = &meta.taken {
        ifd0.ascii(306, t);
        let mut exif = Ifd::new();
        exif.ascii(0x9003, t);
        ifd0.set(0x8769, Val::Ifds(vec![exif]));
    }
    ifd0.set(330, Val::Ifds(vec![raw_ifd, preview]));
    ifd0.set(50706, Val::Byte(vec![1, 4, 0, 0]));
    ifd0.set(50707, Val::Byte(vec![1, 1, 0, 0]));
    let model = match (meta.make, meta.model) {
        (Some(a), Some(b)) => format!("{a} {b}"),
        _ => "Owlbum Fixture".to_string(),
    };
    ifd0.ascii(50708, &model);
    let identity: Vec<[i32; 2]> = (0..9)
        .map(|i| [if i % 4 == 0 { 10000 } else { 0 }, 10000])
        .collect();
    ifd0.set(50721, Val::SRational(identity));
    ifd0.set(50728, Val::Rational(vec![[1, 1], [1, 1], [1, 1]]));
    ifd0.short(50778, 21); // D65
    tiff::write(&ifd0)
}
