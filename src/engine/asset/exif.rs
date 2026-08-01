//! Minimal EXIF orientation reader for JPEG.
//!
//! Re-encoding a JPEG strips its metadata, including the Orientation tag a
//! camera used instead of rotating pixels, so the optimizer must bake that
//! rotation into the pixels first or phone photos come out sideways. This
//! reads just the one tag; anything missing or malformed is [`Upright`].

use image::DynamicImage;

/// An EXIF Orientation (tag `0x0112`) value: the transform that makes the
/// stored pixels display upright.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Orientation {
    #[default]
    Upright,
    MirrorH,
    Rotate180,
    MirrorV,
    Transpose,
    Rotate90,
    Transverse,
    Rotate270,
}

impl Orientation {
    /// The orientation recorded in a JPEG's EXIF data, or [`Upright`] when
    /// absent or unreadable (best-effort: never fails an optimization).
    ///
    /// [`Upright`]: Orientation::Upright
    pub fn of_jpeg(bytes: &[u8]) -> Self {
        Jpeg(bytes).orientation().unwrap_or_default()
    }

    /// Apply this orientation to a decoded image so it displays upright
    /// without the metadata.
    pub fn upright(self, img: DynamicImage) -> DynamicImage {
        match self {
            Self::Upright => img,
            Self::MirrorH => img.fliph(),
            Self::Rotate180 => img.rotate180(),
            Self::MirrorV => img.flipv(),
            Self::Transpose => img.rotate90().fliph(),
            Self::Rotate90 => img.rotate90(),
            Self::Transverse => img.rotate90().flipv(),
            Self::Rotate270 => img.rotate270(),
        }
    }

    /// The orientation for an EXIF tag value (`1`–`8`).
    fn of_tag(value: u16) -> Option<Self> {
        [
            Self::Upright,
            Self::MirrorH,
            Self::Rotate180,
            Self::MirrorV,
            Self::Transpose,
            Self::Rotate90,
            Self::Transverse,
            Self::Rotate270,
        ]
        .get(usize::from(value).checked_sub(1)?)
        .copied()
    }
}

/// A JPEG byte stream, walked segment by segment for its EXIF APP1 payload.
struct Jpeg<'a>(&'a [u8]);

impl Jpeg<'_> {
    const SOI: [u8; 2] = [0xFF, 0xD8];
    const APP1: u8 = 0xE1;
    const SOS: u8 = 0xDA;

    fn orientation(&self) -> Option<Orientation> {
        let tiff = self.exif()?;
        Orientation::of_tag(Tiff::new(tiff)?.short(0x0112)?)
    }

    /// The TIFF payload of the first `APP1` segment carrying EXIF data.
    fn exif(&self) -> Option<&[u8]> {
        let b = self.0;
        if b.get(..2)? != Self::SOI {
            return None;
        }
        let mut i = 2;
        while i + 4 <= b.len() {
            if b[i] != 0xFF {
                return None;
            }
            let marker = b[i + 1];
            // metadata lives before the scan; standalone markers carry no length
            if marker == Self::SOS {
                return None;
            }
            if (0xD0..=0xD8).contains(&marker) || marker == 0x01 {
                i += 2;
                continue;
            }
            let len = usize::from(u16::from_be_bytes([b[i + 2], b[i + 3]]));
            let segment = b.get(i + 4..i + 2 + len)?;
            if marker == Self::APP1
                && let Some(tiff) = segment.strip_prefix(b"Exif\0\0")
            {
                return Some(tiff);
            }
            i += 2 + len;
        }
        None
    }
}

/// A TIFF structure (as embedded in EXIF), read with its declared byte order.
struct Tiff<'a> {
    bytes: &'a [u8],
    be: bool,
}

impl<'a> Tiff<'a> {
    fn new(bytes: &'a [u8]) -> Option<Self> {
        let be = match bytes.get(..2)? {
            b"MM" => true,
            b"II" => false,
            _ => return None,
        };
        let tiff = Self { bytes, be };
        (tiff.u16(2)? == 42).then_some(tiff)
    }

    /// The inline `SHORT` value of `tag` in IFD0, if present.
    fn short(&self, tag: u16) -> Option<u16> {
        let ifd = usize::try_from(self.u32(4)?).ok()?;
        let count = usize::from(self.u16(ifd)?);
        (0..count)
            .map(|n| ifd + 2 + n * 12)
            .find(|&entry| self.u16(entry) == Some(tag))
            // entry layout: tag(2) type(2) count(4) value(4, SHORT stored inline)
            .and_then(|entry| self.u16(entry + 8))
    }

    fn u16(&self, at: usize) -> Option<u16> {
        let raw = self.bytes.get(at..at + 2)?.try_into().ok()?;
        Some(if self.be {
            u16::from_be_bytes(raw)
        } else {
            u16::from_le_bytes(raw)
        })
    }

    fn u32(&self, at: usize) -> Option<u32> {
        let raw = self.bytes.get(at..at + 4)?.try_into().ok()?;
        Some(if self.be {
            u32::from_be_bytes(raw)
        } else {
            u32::from_le_bytes(raw)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Jpeg, Orientation};

    /// A minimal JPEG prefix: SOI + one EXIF APP1 whose TIFF holds just the
    /// orientation tag, in the given byte order.
    fn jpeg(orientation: u16, be: bool) -> Vec<u8> {
        let mut tiff: Vec<u8> = if be {
            [b"MM".as_slice(), &42u16.to_be_bytes(), &8u32.to_be_bytes()].concat()
        } else {
            [b"II".as_slice(), &42u16.to_le_bytes(), &8u32.to_le_bytes()].concat()
        };
        let word = |v: u16| if be { v.to_be_bytes() } else { v.to_le_bytes() };
        let long = |v: u32| if be { v.to_be_bytes() } else { v.to_le_bytes() };
        tiff.extend(word(1)); // one IFD0 entry
        tiff.extend(word(0x0112)); // orientation tag
        tiff.extend(word(3)); // type SHORT
        tiff.extend(long(1)); // count
        tiff.extend(word(orientation));
        tiff.extend(word(0)); // value padding to 4 bytes
        let payload = [b"Exif\0\0".as_slice(), &tiff].concat();
        let mut out = vec![0xFF, 0xD8, 0xFF, 0xE1];
        out.extend(
            u16::try_from(payload.len() + 2)
                .expect("a fixture's EXIF payload fits an APP1 length")
                .to_be_bytes(),
        );
        out.extend(payload);
        out
    }

    #[test]
    fn reads_orientation_in_both_byte_orders() {
        assert_eq!(Orientation::of_jpeg(&jpeg(6, false)), Orientation::Rotate90);
        assert_eq!(Orientation::of_jpeg(&jpeg(6, true)), Orientation::Rotate90);
        assert_eq!(Orientation::of_jpeg(&jpeg(1, true)), Orientation::Upright);
        assert_eq!(
            Orientation::of_jpeg(&jpeg(8, false)),
            Orientation::Rotate270
        );
    }

    #[test]
    fn missing_or_malformed_exif_is_upright() {
        assert_eq!(Orientation::of_jpeg(b""), Orientation::Upright);
        assert_eq!(
            Orientation::of_jpeg(&[0xFF, 0xD8, 0xFF, 0xDA, 0, 4, 0, 0]),
            Orientation::Upright
        );
        assert_eq!(Orientation::of_jpeg(&jpeg(9, false)), Orientation::Upright);
        assert!(Jpeg(&jpeg(6, false)[..8]).orientation().is_none());
    }
}
