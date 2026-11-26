//! A small ICC v2 display profile with Display P3 primaries, for the
//! wide-gamut fixture. Built by hand so no profile file is committed.

fn be32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn xyz(v: [i32; 3]) -> Vec<u8> {
    let mut t = b"XYZ \0\0\0\0".to_vec();
    for c in v {
        t.extend_from_slice(&c.to_be_bytes());
    }
    t
}

fn desc(text: &str) -> Vec<u8> {
    let mut t = b"desc\0\0\0\0".to_vec();
    be32(&mut t, u32::try_from(text.len() + 1).expect("text"));
    t.extend_from_slice(text.as_bytes());
    t.push(0);
    t.extend_from_slice(&[0; 4 + 4 + 2 + 1 + 67]); // no Unicode, no ScriptCode
    t
}

fn text(text: &str) -> Vec<u8> {
    let mut t = b"text\0\0\0\0".to_vec();
    t.extend_from_slice(text.as_bytes());
    t.push(0);
    t
}

/// A gamma curve; `gamma_x256` is the gamma as u8Fixed8 (563 is 2.2).
fn curve(gamma_x256: u16) -> Vec<u8> {
    let mut t = b"curv\0\0\0\0".to_vec();
    be32(&mut t, 1);
    t.extend_from_slice(&gamma_x256.to_be_bytes());
    t
}

/// Display P3 primaries adapted to D50 (as s15Fixed16), with a 2.2 gamma.
pub fn display_p3() -> Vec<u8> {
    let trc = curve(563);
    let tags: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"desc", desc("Owlbum test Display P3")),
        (b"cprt", text("No copyright, use freely")),
        (b"wtpt", xyz([63190, 65536, 54061])),
        (b"rXYZ", xyz([33758, 15807, -72])),
        (b"gXYZ", xyz([19137, 45364, 2746])),
        (b"bXYZ", xyz([10296, 4365, 51387])),
        (b"rTRC", trc.clone()),
        (b"gTRC", trc.clone()),
        (b"bTRC", trc),
    ];

    let table_len = 4 + 12 * tags.len();
    let mut data = Vec::new();
    let mut table = Vec::new();
    be32(&mut table, u32::try_from(tags.len()).expect("tags"));
    for (sig, bytes) in &tags {
        while data.len() % 4 != 0 {
            data.push(0);
        }
        let offset = 128 + table_len + data.len();
        table.extend_from_slice(*sig);
        be32(&mut table, u32::try_from(offset).expect("offset"));
        be32(&mut table, u32::try_from(bytes.len()).expect("size"));
        data.extend_from_slice(bytes);
    }
    while data.len() % 4 != 0 {
        data.push(0);
    }

    let size = 128 + table_len + data.len();
    let mut out = Vec::with_capacity(size);
    be32(&mut out, u32::try_from(size).expect("size"));
    be32(&mut out, 0); // preferred CMM
    be32(&mut out, 0x0210_0000); // version 2.1
    out.extend_from_slice(b"mntrRGB XYZ ");
    for v in [2025u16, 1, 1, 0, 0, 0] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out.extend_from_slice(b"acsp");
    out.extend_from_slice(&[0; 4 + 4 + 4 + 4 + 8]); // platform, flags, maker, model, attributes
    be32(&mut out, 0); // perceptual intent
    for c in [63190i32, 65536, 54061] {
        out.extend_from_slice(&c.to_be_bytes());
    }
    out.extend_from_slice(&[0; 4 + 44]); // creator, ID and reserved
    assert_eq!(out.len(), 128);
    out.extend_from_slice(&table);
    out.extend_from_slice(&data);
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn header_is_sound() {
        let p = super::display_p3();
        assert_eq!(p.len() % 4, 0);
        assert_eq!(
            u32::from_be_bytes(p[..4].try_into().unwrap()) as usize,
            p.len()
        );
        assert_eq!(&p[36..40], b"acsp");
        assert_eq!(&p[12..20], b"mntrRGB ");
    }
}
