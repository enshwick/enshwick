//! Item icons: an icon of the mod's own for an item, recoloured from a
//! vanilla icon at run time or drawn from a PNG the mod embeds.
//!
//! The game's item icons are pictures it ships ready-made, UI images named
//! by GUID, whose pixels it reads from its archive. An [`Icon`] is a UI
//! image the mod serves under a GUID of its own and the item names in its
//! `iconImage`. Its pixels come from the client's own archive, recoloured
//! with a [`Tint`], or from a PNG: no game data ships with the mod.
//!
//! Icons show on a client whose loader knows this build's texture code; the
//! item keeps its vanilla icon anywhere else, the dedicated server
//! included, where no icon shows anyway.

use crate::abi::HostApi;
use crate::{Load, Off};

use super::{Blob, EngineFacts, Guid, Id, types};

/// The widest and tallest PNG an icon may be, in pixels; a larger one does
/// not decode.
pub const MAX_SIDE: u32 = 2048;

/// An item's own icon, for [`NewItem::icon`](super::items::NewItem::icon).
///
/// # Examples
///
/// ```
/// use enshwick::content::{Guid, Id, id};
/// use enshwick::content::icons::{Icon, Pixels, Tint};
///
/// let ghostly = Icon {
///     id: id!("ghostly-staff.icon"),
///     like: Guid::parse("505ccfbc-440d-4d63-a3d6-d689d3d55e78"),
///     pixels: Pixels::Tint(Tint { colour: [120, 170, 255], amount: 60 }),
/// };
/// assert!(matches!(ghostly.pixels, Pixels::Tint(_)));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Icon {
    /// The icon's id, the mod's own.
    pub id: Id,
    /// The vanilla icon it starts from: an item's GUID for an item drawn
    /// from its model, or the GUID in an item's `iconImage`.
    pub like: Guid,
    /// Where its pixels come from.
    pub pixels: Pixels,
}

/// Where an [`Icon`]'s pixels come from.
///
/// # Examples
///
/// See [`Icon`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Pixels {
    /// The vanilla icon's pixels, recoloured.
    Tint(Tint),
    /// A PNG the mod embeds, with `include_bytes!`; any size, best square,
    /// such as 256 by 256.
    Png(&'static [u8]),
}

/// A recolouring: every pixel moves `amount` percent of the way toward
/// `colour` at the pixel's own brightness, so shading stays and the hue
/// changes; alpha stays.
///
/// # Examples
///
/// ```
/// use enshwick::content::icons::{Tint, tint};
///
/// let mut white = [255, 255, 255, 200];
/// tint(&mut white, Tint { colour: [0, 128, 255], amount: 100 });
/// assert_eq!(white, [0, 128, 255, 200]);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Tint {
    /// The colour, red, green and blue.
    pub colour: [u8; 3],
    /// How far each pixel moves toward it, 0 to 100.
    pub amount: u8,
}

/// Recolours `rgba`, pixels of red, green, blue and alpha, with `tint`.
///
/// # Examples
///
/// See [`Tint`].
pub fn tint(rgba: &mut [u8], tint: Tint) {
    let amount = u32::from(tint.amount.min(100));
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let [r, g, b] = [pixel[0], pixel[1], pixel[2]].map(u32::from);
        // Rec. 601 luma in 0..=255, scaled by 1000.
        let luma = 299 * r + 587 * g + 114 * b;
        for (channel, &target) in pixel[..3].iter_mut().zip(&tint.colour) {
            let toward = luma * u32::from(target) / (1000 * 255);
            let from = u32::from(*channel);
            let mixed = (from * (100 - amount) + toward * amount + 50) / 100;
            *channel = u8::try_from(mixed.min(255)).unwrap_or(u8::MAX);
        }
    }
}

/// Pixel formats of the game's UI images (`PixelFormat`).
pub mod formats {
    /// Red, green, blue and alpha, a byte each.
    pub const RGBA8: u8 = 0x25;
    /// The same, sRGB.
    pub const RGBA8_SRGB: u8 = 0x2b;
    /// BC1 with alpha.
    pub const BC1: u8 = 0x85;
    /// BC1 with alpha, sRGB.
    pub const BC1_SRGB: u8 = 0x86;
    /// BC3.
    pub const BC3: u8 = 0x8b;
    /// BC3, sRGB.
    pub const BC3_SRGB: u8 = 0x8c;
    /// BC7, the format of most item icons.
    pub const BC7: u8 = 0x91;
    /// BC7, sRGB.
    pub const BC7_SRGB: u8 = 0x92;
}

/// The first mip level of a UI image's pixels as red, green, blue and
/// alpha bytes; `None` for a format it does not read or data too short.
///
/// # Examples
///
/// ```
/// use enshwick::content::icons::{decode, formats};
///
/// let pixels = decode(formats::RGBA8, 1, 1, &[1, 2, 3, 4]).ok_or("no pixels")?;
/// assert_eq!(pixels, [1, 2, 3, 4]);
/// # Ok::<(), &str>(())
/// ```
#[must_use]
pub fn decode(format: u8, width: u32, height: u32, data: &[u8]) -> Option<Vec<u8>> {
    let (w, h) = (usize::try_from(width).ok()?, usize::try_from(height).ok()?);
    let size = w.checked_mul(h)?.checked_mul(4)?;
    match format {
        formats::RGBA8 | formats::RGBA8_SRGB => data.get(..size).map(<[u8]>::to_vec),
        formats::BC1 | formats::BC1_SRGB => blocks(w, h, 8, data, bcdec_rs::bc1),
        formats::BC3 | formats::BC3_SRGB => blocks(w, h, 16, data, bcdec_rs::bc3),
        formats::BC7 | formats::BC7_SRGB => blocks(w, h, 16, data, bcdec_rs::bc7),
        _ => None,
    }
}

/// Decodes 4 by 4 blocks of `block` bytes each into a `w` by `h` image.
fn blocks(
    width: usize,
    height: usize,
    block: usize,
    data: &[u8],
    decode: fn(&[u8], &mut [u8], usize),
) -> Option<Vec<u8>> {
    let (across, down) = (width.div_ceil(4), height.div_ceil(4));
    if data.len() < across.checked_mul(down)?.checked_mul(block)? {
        return None;
    }
    let mut out = vec![0u8; width * height * 4];
    let mut tile = [0u8; 64];
    for block_y in 0..down {
        for block_x in 0..across {
            let at = (block_y * across + block_x) * block;
            decode(&data[at..at + block], &mut tile, 16);
            for row in 0..4 {
                let line = block_y * 4 + row;
                if line >= height {
                    break;
                }
                let column = block_x * 4;
                let bytes = (width - column).min(4) * 4;
                let to = (line * width + column) * 4;
                out[to..to + bytes].copy_from_slice(&tile[row * 16..row * 16 + bytes]);
            }
        }
    }
    Some(out)
}

/// The pixels of a PNG as red, green, blue and alpha bytes, with its width
/// and height; `None` for data that does not decode.
///
/// # Examples
///
/// ```
/// assert!(enshwick::content::icons::decode_png(b"not a png").is_none());
/// ```
#[must_use]
pub fn decode_png(png: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    use zune_core::bytestream::ZCursor;
    use zune_core::colorspace::ColorSpace;
    use zune_core::options::DecoderOptions;

    const SIDE: usize = MAX_SIDE as usize;
    let options = DecoderOptions::default()
        .set_max_width(SIDE)
        .set_max_height(SIDE)
        .png_set_strip_to_8bit(true)
        .png_set_add_alpha_channel(true);
    let mut decoder = zune_png::PngDecoder::new_with_options(ZCursor::new(png), options);
    let bytes = decoder.decode_raw().ok()?;
    let (width, height) = decoder.dimensions()?;
    let rgba: Vec<u8> = match decoder.colorspace()? {
        ColorSpace::RGBA => bytes,
        ColorSpace::LumaA => bytes
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[v, a]| [v, v, v, a])
            .collect(),
        _ => return None,
    };
    (rgba.len() == width.checked_mul(height)?.checked_mul(4)?).then_some((
        rgba,
        u32::try_from(width).ok()?,
        u32::try_from(height).ok()?,
    ))
}

/// The loader's table ids of a UI image's fields, in [`ImageLayout`] order:
/// the type is not reflected, so the ids are the table's alone.
pub(crate) const IMAGE_FIELDS: [&str; 6] = [
    "ui_image.content",
    "ui_image.width",
    "ui_image.height",
    "ui_image.flags",
    "ui_image.format",
    "ui_image.levels",
];

/// Where a UI image keeps its fields, from the loader's table for the
/// running build: the type is not reflected.
///
/// # Examples
///
/// See [`clone_image`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageLayout {
    /// The content hash of its pixels, 16 bytes.
    pub content: usize,
    /// Its width, `u32`.
    pub width: usize,
    /// Its height, `u32`.
    pub height: usize,
    /// Its flags byte.
    pub flags: usize,
    /// Its pixel format byte.
    pub format: usize,
    /// Its mip level count byte.
    pub levels: usize,
}

impl ImageLayout {
    /// The layout on the running build.
    ///
    /// # Errors
    ///
    /// [`Off`] on a build whose table lacks it, such as a dedicated server.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) {
    ///     let known = enshwick::content::icons::ImageLayout::resolve(load).is_ok();
    ///     enshwick::info!("icons on this build: {known}");
    /// }
    /// ```
    pub fn resolve(facts: &(impl EngineFacts + ?Sized)) -> Result<Self, Off> {
        let [content, width, height, flags, format, levels] = IMAGE_FIELDS;
        let at = |id: &str| facts.offset(id);
        Ok(Self {
            content: at(content)?,
            width: at(width)?,
            height: at(height)?,
            flags: at(flags)?,
            format: at(format)?,
            levels: at(levels)?,
        })
    }

    /// The content hash, size, format and levels of the UI image `image`.
    fn read(&self, image: &Blob) -> Option<([u32; 4], u32, u32, u8)> {
        let word = |at: usize| image.read_u32(at);
        let hash = [
            word(self.content)?,
            word(self.content + 4)?,
            word(self.content + 8)?,
            word(self.content + 12)?,
        ];
        let format = *image.bytes().get(self.format)?;
        Some((hash, word(self.width)?, word(self.height)?, format))
    }
}

/// The UI image `base` with its pixels replaced: content `hash`, `width`
/// by `height` of RGBA8 with one mip level. `None` when `base` is too
/// short.
///
/// # Examples
///
/// ```
/// use enshwick::content::icons::{ImageLayout, clone_image};
///
/// let layout = ImageLayout { content: 0, width: 0x28, height: 0x2c, flags: 0x30, format: 0x31, levels: 0x32 };
/// let image = clone_image(&layout, &[0; 0x34], [4, 1, 2, 3], 1, 1).ok_or("too short")?;
/// assert_eq!((image[0x31], image[0x32]), (0x25, 1));
/// # Ok::<(), &str>(())
/// ```
#[must_use]
pub fn clone_image(
    layout: &ImageLayout,
    base: &[u8],
    hash: [u32; 4],
    width: u32,
    height: u32,
) -> Option<Vec<u8>> {
    let mut image = Blob::new(base);
    let mut content = [0u8; 16];
    for (bytes, word) in content.as_chunks_mut::<4>().0.iter_mut().zip(hash) {
        bytes.copy_from_slice(&word.to_le_bytes());
    }
    let written = image.write(layout.content, &content)
        && image.write(layout.width, &width.to_le_bytes())
        && image.write(layout.height, &height.to_le_bytes())
        && image.write(layout.format, &[formats::RGBA8])
        && image.write(layout.levels, &[1]);
    written.then(|| image.into_bytes())
}

/// The game's own content `hash` from its archive.
fn read_content(api: &HostApi, handle: crate::abi::ModHandle, hash: [u32; 4]) -> Option<Vec<u8>> {
    let mut buffer = vec![0u8; usize::try_from(hash[0]).ok()?];
    let mut len = 0u32;
    let status = (api.content_read)(
        handle,
        &raw const hash,
        buffer.as_mut_ptr(),
        u32::try_from(buffer.len()).ok()?,
        &raw mut len,
    );
    status.is_ok().then(|| {
        buffer.truncate(usize::try_from(len).unwrap_or(0));
        buffer
    })
}

/// The pixels of an icon: the base image's own recoloured, or the PNG's.
fn pixels_of(
    api: &HostApi,
    handle: crate::abi::ModHandle,
    layout: &ImageLayout,
    base: &Blob,
    pixels: Pixels,
) -> Option<(Vec<u8>, u32, u32)> {
    match pixels {
        Pixels::Tint(colour) => {
            let (hash, width, height, format) = layout.read(base)?;
            let data = read_content(api, handle, hash)?;
            let mut rgba = decode(format, width, height, &data)?;
            tint(&mut rgba, colour);
            Some((rgba, width, height))
        }
        Pixels::Png(png) => decode_png(png),
    }
}

/// Builds the icon's UI image from the vanilla one in `base`: makes its
/// pixels, serves them as content and names them. When the pixels cannot be
/// made, the vanilla image itself: the icon shows the vanilla picture and
/// the mod stays on.
fn build(layout: &ImageLayout, base: &[u8], pixels: Pixels) -> Vec<u8> {
    let built = (|| {
        let host = crate::host()?;
        let api = host.api();
        let image = Blob::new(base);
        let (rgba, width, height) = pixels_of(api, host.handle(), layout, &image, pixels)?;
        let mut hash = [0u32; 4];
        let status = (api.content_serve)(
            host.handle(),
            rgba.as_ptr(),
            u32::try_from(rgba.len()).ok()?,
            &raw mut hash,
        );
        if !status.is_ok() {
            return None;
        }
        clone_image(layout, base, hash, width, height)
    })();
    if built.is_none() {
        crate::warn!("an icon's pixels could not be made: it shows the vanilla picture");
    }
    built.unwrap_or_else(|| base.to_vec())
}

/// Whether this client shows served icons now.
pub(crate) fn ready() -> bool {
    crate::host().is_some_and(|host| (host.api().icons_ready)(host.handle()).is_ok())
}

impl Load<'_> {
    /// Serves `icon`'s UI image, built from the vanilla one it starts from
    /// when the client first shows it. [`Load::add_item`] does it for an
    /// item's icon.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses the icon's id
    /// (see [`Load::declare`]).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{Guid, Id, id};
    /// use enshwick::content::icons::{Icon, Pixels, Tint};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.add_icon(Icon {
    ///         id: id!("ghostly-staff.icon"),
    ///         like: Guid::parse("505ccfbc-440d-4d63-a3d6-d689d3d55e78"),
    ///         pixels: Pixels::Tint(Tint { colour: [120, 170, 255], amount: 60 }),
    ///     })
    /// }
    /// ```
    pub fn add_icon(&mut self, icon: Icon) -> Result<(), Off> {
        self.declare(icon.id)?;
        // A build without the layout, such as the dedicated server, shows
        // no icons: the item keeps its vanilla one.
        let (Ok(layout), Ok(key), Ok(like)) = (
            ImageLayout::resolve(self),
            icon.id.key(types::UI_TEXTURE_RESOURCE),
            icon.like.key(types::UI_TEXTURE_RESOURCE),
        ) else {
            return Ok(());
        };
        self.serve(key, like, move |base| {
            Some(build(&layout, base, icon.pixels))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_tint_keeps_the_pixels_and_a_full_one_keeps_the_shading() {
        let mut pixels = [10, 200, 30, 255, 128, 128, 128, 7];
        let before = pixels;
        tint(
            &mut pixels,
            Tint {
                colour: [255, 0, 0],
                amount: 0,
            },
        );
        assert_eq!(pixels, before);
        tint(
            &mut pixels,
            Tint {
                colour: [255, 0, 0],
                amount: 100,
            },
        );
        assert_eq!(&pixels[4..], &[128, 0, 0, 7], "grey keeps its brightness");
        assert!(pixels[0] > 0 && pixels[1] == 0 && pixels[2] == 0);
        let mut half = [0, 0, 0, 255];
        tint(
            &mut half,
            Tint {
                colour: [255, 255, 255],
                amount: 50,
            },
        );
        assert_eq!(half, [0, 0, 0, 255], "black stays black");
    }

    #[test]
    fn blocks_cover_an_image_whose_size_is_no_multiple_of_four() {
        // One BC1 block, colour 0 white and colour 1 black, all indices 0.
        let block = [0xff, 0xff, 0x00, 0x00, 0, 0, 0, 0];
        let pixels = decode(formats::BC1, 3, 2, &block).unwrap();
        assert_eq!(pixels.len(), 3 * 2 * 4);
        assert!(pixels.as_chunks::<4>().0.iter().all(|p| *p == [255; 4]));
        assert!(
            decode(formats::BC7, 8, 8, &[0; 16]).is_none(),
            "four blocks needed"
        );
        assert!(decode(0x99, 1, 1, &[0; 16]).is_none());
    }

    #[test]
    fn a_cloned_image_names_the_new_pixels() {
        let layout = ImageLayout {
            content: 0,
            width: 0x28,
            height: 0x2c,
            flags: 0x30,
            format: 0x31,
            levels: 0x32,
        };
        let mut base = vec![0u8; 0x40];
        base[0x30..0x33].copy_from_slice(&[1, formats::BC7, 9]);
        let image = clone_image(&layout, &base, [16, 1, 2, 3], 2, 2).unwrap();
        let blob = Blob::new(&image);
        assert_eq!(
            layout.read(&blob),
            Some(([16, 1, 2, 3], 2, 2, formats::RGBA8))
        );
        assert_eq!((image[0x30], image[0x32]), (1, 1), "flags kept, one level");
        assert!(clone_image(&layout, &[0; 8], [0; 4], 1, 1).is_none());
    }

    #[test]
    fn a_png_decodes_to_rgba() {
        // A 1 by 1 red PNG: signature, IHDR, IDAT, IEND.
        let png: &[u8] = &[
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0xc9, 0xfe, 0x92,
            0xef, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
        ];
        let (rgba, w, h) = decode_png(png).unwrap();
        assert_eq!((w, h), (1, 1));
        assert_eq!(rgba, [255, 0, 0, 255]);
    }

    /// A grey PNG `width` by `height`.
    fn grey_png(width: u32, height: u32) -> Vec<u8> {
        use zune_core::bit_depth::BitDepth;
        use zune_core::colorspace::ColorSpace;
        use zune_core::options::EncoderOptions;

        let (width, height) = (
            usize::try_from(width).unwrap(),
            usize::try_from(height).unwrap(),
        );
        let pixels = vec![0x80; width * height];
        let options = EncoderOptions::new(width, height, ColorSpace::Luma, BitDepth::Eight);
        let mut png = Vec::new();
        zune_png::PngEncoder::new(&pixels, options)
            .encode(&mut png)
            .unwrap();
        png
    }

    #[test]
    fn a_png_up_to_the_icon_bound_decodes_and_a_larger_one_does_not() {
        let (rgba, w, h) = decode_png(&grey_png(MAX_SIDE, 2)).unwrap();
        assert_eq!((w, h), (MAX_SIDE, 2));
        assert_eq!(rgba.len(), 4 * 2 * usize::try_from(MAX_SIDE).unwrap());
        assert!(decode_png(&grey_png(2, MAX_SIDE)).is_some());
        assert!(decode_png(&grey_png(MAX_SIDE + 1, 2)).is_none());
        assert!(decode_png(&grey_png(2, MAX_SIDE + 1)).is_none());
    }
}
