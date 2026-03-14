//! Pixel format descriptors.
//!
//! Defines the supported pixel formats and their properties (bytes per pixel,
//! channel count). Used by all image/video actors for stride calculation and
//! buffer sizing.

/// Supported pixel formats for streaming image data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    /// 4 bytes per pixel: red, green, blue, alpha.
    Rgba8,
    /// 3 bytes per pixel: red, green, blue.
    Rgb8,
    /// 1 byte per pixel: grayscale.
    Gray8,
    /// 2 bytes per pixel: grayscale + alpha.
    GrayAlpha8,
}

impl PixelFormat {
    /// Bytes per pixel.
    pub fn bpp(self) -> usize {
        match self {
            PixelFormat::Rgba8 => 4,
            PixelFormat::Rgb8 => 3,
            PixelFormat::Gray8 => 1,
            PixelFormat::GrayAlpha8 => 2,
        }
    }

    /// Number of channels.
    pub fn channels(self) -> usize {
        self.bpp()
    }

    /// Stride in bytes for a row of `width` pixels.
    pub fn stride(self, width: usize) -> usize {
        width * self.bpp()
    }

    /// Total buffer size for a `width x height` image.
    pub fn buffer_size(self, width: usize, height: usize) -> usize {
        self.stride(width) * height
    }

    /// Map from streaming content type string to pixel format.
    pub fn from_content_type(content_type: &str) -> Option<Self> {
        match content_type {
            "image/raw-rgba" => Some(PixelFormat::Rgba8),
            "image/raw-rgb" => Some(PixelFormat::Rgb8),
            "image/raw-gray" => Some(PixelFormat::Gray8),
            "image/raw-gray-alpha" => Some(PixelFormat::GrayAlpha8),
            _ => None,
        }
    }

    /// Content type string for streaming.
    pub fn content_type(self) -> &'static str {
        match self {
            PixelFormat::Rgba8 => "image/raw-rgba",
            PixelFormat::Rgb8 => "image/raw-rgb",
            PixelFormat::Gray8 => "image/raw-gray",
            PixelFormat::GrayAlpha8 => "image/raw-gray-alpha",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bpp() {
        assert_eq!(PixelFormat::Rgba8.bpp(), 4);
        assert_eq!(PixelFormat::Rgb8.bpp(), 3);
        assert_eq!(PixelFormat::Gray8.bpp(), 1);
        assert_eq!(PixelFormat::GrayAlpha8.bpp(), 2);
    }

    #[test]
    fn test_stride() {
        assert_eq!(PixelFormat::Rgba8.stride(256), 1024);
        assert_eq!(PixelFormat::Gray8.stride(100), 100);
    }

    #[test]
    fn test_buffer_size() {
        assert_eq!(PixelFormat::Rgba8.buffer_size(256, 256), 262144);
    }

    #[test]
    fn test_content_type_roundtrip() {
        for fmt in [
            PixelFormat::Rgba8,
            PixelFormat::Rgb8,
            PixelFormat::Gray8,
            PixelFormat::GrayAlpha8,
        ] {
            let ct = fmt.content_type();
            assert_eq!(PixelFormat::from_content_type(ct), Some(fmt));
        }
    }
}
