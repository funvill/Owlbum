//! A minimal MP4 holding Motion JPEG frames: the same layout ffmpeg writes
//! for `-c:v mjpeg` (`mp4v` sample entry, object type 0x6C). No patented
//! codec is needed to make it, and players that have a JPEG decoder show it.

fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend_from_slice(&u32::try_from(body.len() + 8).expect("box").to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    out
}

fn full(kind: &[u8; 4], version_flags: u32, body: &[u8]) -> Vec<u8> {
    let mut b = version_flags.to_be_bytes().to_vec();
    b.extend_from_slice(body);
    bx(kind, &b)
}

fn be16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn be32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

/// The display matrix: identity, or a quarter turn clockwise.
fn matrix(quarter_turn: bool, stored_h: u32) -> Vec<u8> {
    let one = 0x0001_0000u32;
    let w = 0x4000_0000u32;
    let m: [u32; 9] = if quarter_turn {
        [0, one, 0, one.wrapping_neg(), 0, 0, stored_h << 16, 0, w]
    } else {
        [one, 0, 0, 0, one, 0, 0, 0, w]
    };
    let mut out = Vec::new();
    for v in m {
        be32(&mut out, v);
    }
    out
}

fn descriptor(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = vec![tag, u8::try_from(body.len()).expect("descriptor")];
    out.extend_from_slice(body);
    out
}

/// An MP4 of JPEG `frames`, each shown for `frame_ms`. Every frame has the
/// stored size `w` × `h`; `quarter_turn` marks the track as rotated 90°.
pub fn mjpeg(frames: &[Vec<u8>], w: u16, h: u16, frame_ms: u32, quarter_turn: bool) -> Vec<u8> {
    let n = u32::try_from(frames.len()).expect("frames");
    let duration = n * frame_ms;

    let ftyp = bx(b"ftyp", b"isom\0\0\x02\0isomiso2mp41");
    let mdat_body: Vec<u8> = frames.concat();
    let mdat = bx(b"mdat", &mdat_body);
    let first_frame = u32::try_from(ftyp.len() + 8).expect("offset");

    let mut mvhd = Vec::new();
    be32(&mut mvhd, 0); // created
    be32(&mut mvhd, 0); // modified
    be32(&mut mvhd, 1000); // timescale
    be32(&mut mvhd, duration);
    be32(&mut mvhd, 0x0001_0000); // rate
    be16(&mut mvhd, 0x0100); // volume
    mvhd.extend_from_slice(&[0; 10]);
    mvhd.extend_from_slice(&matrix(false, 0));
    mvhd.extend_from_slice(&[0; 24]);
    be32(&mut mvhd, 2); // next track id
    let mvhd = full(b"mvhd", 0, &mvhd);

    let mut tkhd = Vec::new();
    be32(&mut tkhd, 0);
    be32(&mut tkhd, 0);
    be32(&mut tkhd, 1); // track id
    be32(&mut tkhd, 0);
    be32(&mut tkhd, duration);
    tkhd.extend_from_slice(&[0; 8]);
    be16(&mut tkhd, 0); // layer
    be16(&mut tkhd, 0); // alternate group
    be16(&mut tkhd, 0); // volume
    be16(&mut tkhd, 0);
    tkhd.extend_from_slice(&matrix(quarter_turn, u32::from(h)));
    be32(&mut tkhd, u32::from(w) << 16);
    be32(&mut tkhd, u32::from(h) << 16);
    let tkhd = full(b"tkhd", 3, &tkhd);

    let mut mdhd = Vec::new();
    be32(&mut mdhd, 0);
    be32(&mut mdhd, 0);
    be32(&mut mdhd, 1000);
    be32(&mut mdhd, duration);
    be16(&mut mdhd, 0x55c4); // "und"
    be16(&mut mdhd, 0);
    let mdhd = full(b"mdhd", 0, &mdhd);

    let mut hdlr = vec![0; 4];
    hdlr.extend_from_slice(b"vide");
    hdlr.extend_from_slice(&[0; 12]);
    hdlr.extend_from_slice(b"VideoHandler\0");
    let hdlr = full(b"hdlr", 0, &hdlr);

    let vmhd = full(b"vmhd", 1, &[0; 8]);
    let dref = full(
        b"dref",
        0,
        &[&1u32.to_be_bytes()[..], &full(b"url ", 1, &[])].concat(),
    );
    let dinf = bx(b"dinf", &dref);

    let mut dcd = vec![0x6c, 0x11]; // JPEG, visual stream
    dcd.extend_from_slice(&[0, 0, 0]); // buffer size
    let largest = frames.iter().map(Vec::len).max().unwrap_or(0);
    let bitrate = u32::try_from(largest * 8 * 1000 / frame_ms.max(1) as usize).expect("rate");
    be32(&mut dcd, bitrate);
    be32(&mut dcd, bitrate);
    let mut es = vec![0, 1, 0]; // ES id 1, no flags
    es.extend_from_slice(&descriptor(0x04, &dcd));
    es.extend_from_slice(&descriptor(0x06, &[0x02]));
    let esds = full(b"esds", 0, &descriptor(0x03, &es));

    let mut entry = vec![0; 6];
    be16(&mut entry, 1); // data reference index
    entry.extend_from_slice(&[0; 16]);
    be16(&mut entry, w);
    be16(&mut entry, h);
    be32(&mut entry, 0x0048_0000);
    be32(&mut entry, 0x0048_0000);
    be32(&mut entry, 0);
    be16(&mut entry, 1); // frames per sample
    let mut name = [0u8; 32];
    name[0] = 5;
    name[1..6].copy_from_slice(b"MJPEG");
    entry.extend_from_slice(&name);
    be16(&mut entry, 0x18);
    be16(&mut entry, 0xffff);
    entry.extend_from_slice(&esds);
    let stsd = full(
        b"stsd",
        0,
        &[&1u32.to_be_bytes()[..], &bx(b"mp4v", &entry)].concat(),
    );

    let mut stts = Vec::new();
    be32(&mut stts, 1);
    be32(&mut stts, n);
    be32(&mut stts, frame_ms);
    let stts = full(b"stts", 0, &stts);
    let mut stsc = Vec::new();
    be32(&mut stsc, 1);
    be32(&mut stsc, 1);
    be32(&mut stsc, n);
    be32(&mut stsc, 1);
    let stsc = full(b"stsc", 0, &stsc);
    let mut stsz = Vec::new();
    be32(&mut stsz, 0);
    be32(&mut stsz, n);
    for f in frames {
        be32(&mut stsz, u32::try_from(f.len()).expect("frame"));
    }
    let stsz = full(b"stsz", 0, &stsz);
    let mut stco = Vec::new();
    be32(&mut stco, 1);
    be32(&mut stco, first_frame);
    let stco = full(b"stco", 0, &stco);
    let stbl = bx(b"stbl", &[stsd, stts, stsc, stsz, stco].concat());

    let minf = bx(b"minf", &[vmhd, dinf, stbl].concat());
    let mdia = bx(b"mdia", &[mdhd, hdlr, minf].concat());
    let trak = bx(b"trak", &[tkhd, mdia].concat());
    let moov = bx(b"moov", &[mvhd, trak].concat());

    [ftyp, mdat, moov].concat()
}
