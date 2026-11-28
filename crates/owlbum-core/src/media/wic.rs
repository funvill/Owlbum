//! The Windows Imaging Component as Owlbum's system decoder for HEIC and
//! AVIF (docs/03-media-and-scanning.md, "The system image handler").
//!
//! Windows reads both with the decoder of the Microsoft Store's "HEIF Image
//! Extensions", which hands the pictures to a Media Foundation video
//! decoder: HEVC ("HEVC Video Extensions from Device Manufacturer") or AV1
//! ("AV1 Video Extension"). A codec counts as present when a WIC decoder
//! claims the file extension and a Media Foundation decoder for its video
//! format is registered, as Microsoft's HEIF codec page advises.

#![allow(unsafe_code)]

use std::path::Path;

use windows::Win32::Foundation::GENERIC_READ;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppRGBA, IWICBitmapDecoder,
    IWICBitmapDecoderInfo, IWICBitmapFrameDecode, IWICColorContext, IWICImagingFactory,
    IWICPixelFormatInfo2, WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom,
    WICColorContextProfile, WICComponentEnumerateDefault, WICDecodeMetadataCacheOnDemand,
    WICDecoder,
};
use windows::Win32::Media::MediaFoundation::{
    IMFActivate, MFMediaType_Video, MFT_CATEGORY_VIDEO_DECODER, MFT_ENUM_FLAG_ALL,
    MFT_REGISTER_TYPE_INFO, MFTEnumEx, MFVideoFormat_AV1, MFVideoFormat_HEVC,
};
use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PropVariantClear};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize,
};
use windows::Win32::System::Variant::VT_UI2;
use windows::core::{GUID, HSTRING, IUnknown, Interface};

use super::handler::MediaError;
use super::registry::FORMATS;
use super::system::{Codecs, Decoded, Header, SystemCodec, SystemDecoder};

/// The WIC decoder, with the codecs found when the program started.
#[derive(Debug)]
pub struct Wic {
    codecs: Codecs,
}

impl Wic {
    /// Asks Windows which codecs it has.
    pub fn detect() -> Self {
        let codecs = with_com(|| {
            // SAFETY: COM is initialised for this thread by `with_com`.
            unsafe {
                let exts = decoder_extensions(&factory()?);
                // A WIC decoder claims one of the format's extensions.
                let claimed = |codec: SystemCodec| {
                    FORMATS
                        .iter()
                        .filter(|f| f.system == Some(codec))
                        .flat_map(|f| f.extensions)
                        .any(|e| exts.iter().any(|x| x.eq_ignore_ascii_case(e)))
                };
                Some(Codecs {
                    heic: claimed(SystemCodec::Heic) && has_video_decoder(MFVideoFormat_HEVC),
                    avif: claimed(SystemCodec::Avif) && has_video_decoder(MFVideoFormat_AV1),
                })
            }
        })
        .unwrap_or_default();
        Self { codecs }
    }
}

/// Runs `f` with COM initialised for this thread, and releases COM after
/// only when this call initialised it.
fn with_com<T>(f: impl FnOnce() -> Option<T>) -> Option<T> {
    // SAFETY: balanced initialise and uninitialise on this thread; every
    // interface `f` makes is dropped inside it.
    unsafe {
        let init = CoInitializeEx(None, COINIT_MULTITHREADED);
        let out = f();
        if init.is_ok() {
            CoUninitialize();
        }
        out
    }
}

unsafe fn factory() -> Option<IWICImagingFactory> {
    // SAFETY: COM is initialised by the caller.
    unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok() }
}

/// Every file extension a WIC decoder on this computer claims, packaged
/// Store codecs included.
unsafe fn decoder_extensions(factory: &IWICImagingFactory) -> Vec<String> {
    let mut out = Vec::new();
    // SAFETY: COM is initialised by the caller; buffers are sized as given.
    unsafe {
        let Ok(list) = factory
            .CreateComponentEnumerator(WICDecoder.0 as u32, WICComponentEnumerateDefault.0 as u32)
        else {
            return out;
        };
        loop {
            let mut item: [Option<IUnknown>; 1] = [None];
            let mut fetched = 0u32;
            if list.Next(&mut item, Some(&raw mut fetched)).is_err() || fetched == 0 {
                break;
            }
            let Some(info) = item[0]
                .take()
                .and_then(|u| u.cast::<IWICBitmapDecoderInfo>().ok())
            else {
                continue;
            };
            let mut len = 0u32;
            let _ = info.GetFileExtensions(&mut [], &raw mut len);
            let mut buf = vec![0u16; len as usize];
            if info.GetFileExtensions(&mut buf, &raw mut len).is_ok() {
                let text = String::from_utf16_lossy(&buf);
                out.extend(
                    text.trim_end_matches('\0')
                        .split(',')
                        .map(|e| e.trim().to_owned()),
                );
            }
        }
    }
    out
}

/// True when Media Foundation has a decoder for the video format.
fn has_video_decoder(subtype: GUID) -> bool {
    let input = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: subtype,
    };
    // SAFETY: MFTEnumEx fills a CoTaskMem array of `count` activation
    // objects; each is released (dropped) and the array freed.
    unsafe {
        let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut count = 0u32;
        let found = MFTEnumEx(
            MFT_CATEGORY_VIDEO_DECODER,
            MFT_ENUM_FLAG_ALL,
            Some(&raw const input),
            None,
            &raw mut list,
            &raw mut count,
        )
        .is_ok()
            && count > 0;
        if !list.is_null() {
            for i in 0..count as usize {
                drop(list.add(i).read());
            }
            CoTaskMemFree(Some(list.cast_const().cast()));
        }
        found
    }
}

/// The open decoder and the first frame of `file`.
unsafe fn open(
    factory: &IWICImagingFactory,
    file: &Path,
) -> windows::core::Result<(IWICBitmapDecoder, IWICBitmapFrameDecode)> {
    // SAFETY: COM is initialised by the caller.
    unsafe {
        let decoder = factory.CreateDecoderFromFilename(
            &HSTRING::from(file.as_os_str()),
            None,
            GENERIC_READ,
            WICDecodeMetadataCacheOnDemand,
        )?;
        let frame = decoder.GetFrame(0)?;
        Ok((decoder, frame))
    }
}

/// The orientation Windows reports for a HEIF picture
/// (`WICHeifOrientation`, numbered as EXIF), if any.
unsafe fn orientation(frame: &IWICBitmapFrameDecode) -> Option<u16> {
    // SAFETY: COM is initialised by the caller; the PROPVARIANT is cleared.
    unsafe {
        let reader = frame.GetMetadataQueryReader().ok()?;
        for name in ["/heifProps/{ushort=1}", "System.Photo.Orientation"] {
            let mut value = PROPVARIANT::default();
            let got = reader
                .GetMetadataByName(&HSTRING::from(name), &raw mut value)
                .is_ok();
            let v = &value.Anonymous.Anonymous;
            let o = (got && v.vt == VT_UI2).then(|| v.Anonymous.uiVal);
            let _ = PropVariantClear(&raw mut value);
            if let Some(o) = o.filter(|o| (1..=8).contains(o)) {
                return Some(o);
            }
        }
        None
    }
}

unsafe fn header_of(factory: &IWICImagingFactory, frame: &IWICBitmapFrameDecode) -> Option<Header> {
    // SAFETY: COM is initialised by the caller.
    unsafe {
        let (mut width, mut height) = (0u32, 0u32);
        frame.GetSize(&raw mut width, &raw mut height).ok()?;
        let has_alpha = frame
            .GetPixelFormat()
            .ok()
            .and_then(|f| factory.CreateComponentInfo(&raw const f).ok())
            .and_then(|i| i.cast::<IWICPixelFormatInfo2>().ok())
            .and_then(|i| i.SupportsTransparency().ok())
            .is_some_and(|b| b.as_bool());
        Some(Header {
            width,
            height,
            has_alpha,
            orientation: orientation(frame),
        })
    }
}

/// The first embedded ICC profile of the frame.
unsafe fn icc(factory: &IWICImagingFactory, frame: &IWICBitmapFrameDecode) -> Option<Vec<u8>> {
    // SAFETY: COM is initialised by the caller; buffers are sized as given.
    unsafe {
        let mut count = 0u32;
        frame.GetColorContexts(&mut [], &raw mut count).ok()?;
        if count == 0 {
            return None;
        }
        let mut contexts: Vec<Option<IWICColorContext>> = Vec::with_capacity(count as usize);
        for _ in 0..count {
            contexts.push(Some(factory.CreateColorContext().ok()?));
        }
        frame.GetColorContexts(&mut contexts, &raw mut count).ok()?;
        contexts.into_iter().flatten().find_map(|c| {
            if c.GetType().ok()? != WICColorContextProfile {
                return None;
            }
            let mut len = 0u32;
            c.GetProfileBytes(&mut [], &raw mut len).ok()?;
            let mut buf = vec![0u8; len as usize];
            c.GetProfileBytes(&mut buf, &raw mut len).ok()?;
            (!buf.is_empty()).then_some(buf)
        })
    }
}

fn decode_error() -> MediaError {
    // HEIC and AVIF files share the decoder; the handler puts its own
    // format's label in the sentence.
    MediaError::decode("HEIF")
}

impl SystemDecoder for Wic {
    fn codecs(&self) -> Codecs {
        self.codecs
    }

    fn header(&self, file: &Path) -> Result<Header, MediaError> {
        with_com(|| {
            // SAFETY: COM is initialised by `with_com`; the decoder (which
            // holds the file open) is dropped before returning.
            unsafe {
                let factory = factory()?;
                let (_decoder, frame) = open(&factory, file).ok()?;
                header_of(&factory, &frame)
            }
        })
        .ok_or_else(decode_error)
    }

    fn decode(&self, file: &Path) -> Result<Decoded, MediaError> {
        with_com(|| {
            // SAFETY: as `header`; the buffer is sized for the stride and
            // height given to CopyPixels.
            unsafe {
                let factory = factory()?;
                let (_decoder, frame) = open(&factory, file).ok()?;
                let header = header_of(&factory, &frame)?;
                let converter = factory.CreateFormatConverter().ok()?;
                converter
                    .Initialize(
                        &frame,
                        &GUID_WICPixelFormat32bppRGBA,
                        WICBitmapDitherTypeNone,
                        None,
                        0.0,
                        WICBitmapPaletteTypeCustom,
                    )
                    .ok()?;
                let stride = header.width.checked_mul(4)?;
                let size = usize::try_from(u64::from(stride) * u64::from(header.height)).ok()?;
                let mut rgba = vec![0u8; size];
                converter
                    .CopyPixels(std::ptr::null(), stride, &mut rgba)
                    .ok()?;
                Some(Decoded {
                    header,
                    rgba,
                    icc: icc(&factory, &frame),
                })
            }
        })
        .ok_or_else(decode_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::system::SystemImage;

    /// What this computer has. Run with `--ignored --nocapture` to see it.
    #[test]
    #[ignore = "asks the real Windows codecs; prints what it found"]
    fn detect_reports_the_codecs_of_this_computer() {
        let wic = Wic::detect();
        println!("system codecs: {:?}", wic.codecs);
        with_com(|| {
            // SAFETY: COM is initialised by `with_com`.
            unsafe {
                let exts = decoder_extensions(&factory()?);
                println!("WIC decoder extensions: {}", exts.join(" "));
            }
            Some(())
        });
        println!("HEVC decoder: {}", has_video_decoder(MFVideoFormat_HEVC));
        println!("AV1 decoder: {}", has_video_decoder(MFVideoFormat_AV1));
    }

    /// Reads a real file through Windows: set `OWLBUM_HEIC_SAMPLE` or
    /// `OWLBUM_AVIF_SAMPLE` to a photo's path (outside the repository) on
    /// a computer with the Store extensions, and run with `--ignored`.
    #[test]
    #[ignore = "needs the Microsoft Store codecs and a sample file"]
    fn a_real_sample_decodes_upright_in_srgb() {
        let wic = Wic::detect();
        let mut ran = false;
        for (var, handler, has) in [
            (
                "OWLBUM_HEIC_SAMPLE",
                &super::super::system::HEIC,
                wic.codecs.heic,
            ),
            (
                "OWLBUM_AVIF_SAMPLE",
                &super::super::system::AVIF,
                wic.codecs.avif,
            ),
        ] {
            let Some(path) = std::env::var_os(var) else {
                continue;
            };
            assert!(has, "{var} is set but Windows lacks the codec");
            let handler: &SystemImage = handler;
            let meta = handler.metadata_with(&wic, Path::new(&path)).unwrap();
            let px = handler.working_with(&wic, Path::new(&path)).unwrap();
            println!("{var}: metadata {meta:?}");
            println!("{var}: pixels {px:?}");
            assert_eq!((px.width, px.height), (meta.width, meta.height));
            assert!(px.rgba.iter().any(|&b| b != 0), "the picture is all black");
            ran = true;
        }
        assert!(ran, "set OWLBUM_HEIC_SAMPLE or OWLBUM_AVIF_SAMPLE");
    }
}
