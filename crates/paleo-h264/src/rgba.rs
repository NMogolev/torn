//! YUV 4:2:0 to RGBA conversion.
//!
//! The conversion is integer fixed-point so the result is deterministic and
//! independent of floating-point rounding. Coefficients use the limited-range
//! BT.601 or BT.709 matrix, which is what SD and HD video normally carry.

use rusty_h264_common::YuvFrame;

/// The luma/chroma matrix used to convert YUV to RGB.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ColorMatrix {
    /// BT.601 for standard definition, BT.709 for 720 lines and above.
    #[default]
    Auto,
    /// BT.601, the matrix used by most standard-definition video.
    Bt601,
    /// BT.709, the matrix used by most high-definition video.
    Bt709,
}

impl ColorMatrix {
    fn resolve(self, height: usize) -> Coefficients {
        let high_definition = match self {
            Self::Bt601 => false,
            Self::Bt709 => true,
            Self::Auto => height >= 720,
        };
        if high_definition {
            Coefficients::BT_709
        } else {
            Coefficients::BT_601
        }
    }
}

/// Fixed-point coefficients scaled by 1024.
struct Coefficients {
    luma: i32,
    r_v: i32,
    g_u: i32,
    g_v: i32,
    b_u: i32,
}

impl Coefficients {
    // Limited-range BT.601: R = 1.164(Y-16) + 1.596(V-128), and so on.
    const BT_601: Self = Self {
        luma: 1192,
        r_v: 1634,
        g_u: 400,
        g_v: 833,
        b_u: 2066,
    };

    // Limited-range BT.709.
    const BT_709: Self = Self {
        luma: 1192,
        r_v: 1836,
        g_u: 218,
        g_v: 546,
        b_u: 2163,
    };
}

/// Converts a decoded 4:2:0 frame into tightly packed RGBA.
///
/// `out` is resized to `width * height * 4` bytes and fully overwritten.
pub(crate) fn convert(frame: &YuvFrame, matrix: ColorMatrix, out: &mut Vec<u8>) {
    let width = frame.width;
    let height = frame.height;
    let Some(pixels) = width
        .checked_mul(height)
        .filter(|_| width > 0 && height > 0)
    else {
        out.clear();
        return;
    };
    out.clear();
    out.resize(pixels * 4, 0);

    let coefficients = matrix.resolve(height);
    let chroma_width = width / 2;

    for row in 0..height {
        let chroma_row = row / 2;
        let Some(y_row) = frame.y.get(row * width..(row + 1) * width) else {
            return;
        };
        let Some(u_row) = frame
            .u
            .get(chroma_row * chroma_width..(chroma_row + 1) * chroma_width)
        else {
            return;
        };
        let Some(v_row) = frame
            .v
            .get(chroma_row * chroma_width..(chroma_row + 1) * chroma_width)
        else {
            return;
        };
        let Some(out_row) = out.get_mut(row * width * 4..(row + 1) * width * 4) else {
            return;
        };

        for x in 0..width {
            let chroma_column = x / 2;
            let Some(&u) = u_row.get(chroma_column) else {
                return;
            };
            let Some(&v) = v_row.get(chroma_column) else {
                return;
            };
            let luma = i32::from(y_row[x]) - 16;
            let u = i32::from(u) - 128;
            let v = i32::from(v) - 128;
            let scaled_luma = coefficients.luma * luma;

            let pixel = &mut out_row[x * 4..x * 4 + 4];
            pixel[0] = clamp8((scaled_luma + coefficients.r_v * v + 512) >> 10);
            pixel[1] =
                clamp8((scaled_luma - coefficients.g_u * u - coefficients.g_v * v + 512) >> 10);
            pixel[2] = clamp8((scaled_luma + coefficients.b_u * u + 512) >> 10);
            pixel[3] = 255;
        }
    }
}

fn clamp8(value: i32) -> u8 {
    u8::try_from(value.clamp(0, 255)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(width: usize, height: usize, y: u8, u: u8, v: u8) -> YuvFrame {
        YuvFrame {
            width,
            height,
            y: vec![y; width * height],
            u: vec![u; (width / 2) * (height / 2)],
            v: vec![v; (width / 2) * (height / 2)],
        }
    }

    #[test]
    fn converts_neutral_grey() {
        let mut out = Vec::new();
        convert(&frame(4, 2, 126, 128, 128), ColorMatrix::Bt601, &mut out);
        assert_eq!(out.len(), 4 * 2 * 4);
        for pixel in out.chunks_exact(4) {
            assert!(pixel[0].abs_diff(128) <= 1, "{pixel:?}");
            assert_eq!(pixel[0], pixel[1]);
            assert_eq!(pixel[1], pixel[2]);
            assert_eq!(pixel[3], 255);
        }
    }

    #[test]
    fn clamps_out_of_range_samples() {
        let mut out = Vec::new();
        // Maximum luma with no blue and maximum red.
        convert(&frame(2, 2, 255, 0, 255), ColorMatrix::Bt709, &mut out);
        for pixel in out.chunks_exact(4) {
            assert_eq!(pixel[0], 255);
            assert_eq!(pixel[1], 238);
            assert_eq!(pixel[2], 8);
            assert_eq!(pixel[3], 255);
        }

        // Blue pushes red below zero and blue above 255.
        convert(&frame(2, 2, 16, 255, 0), ColorMatrix::Bt709, &mut out);
        for pixel in out.chunks_exact(4) {
            assert_eq!(pixel[0], 0);
            assert_eq!(pixel[2], 255);
        }
    }

    #[test]
    fn ignores_degenerate_frames() {
        let mut out = vec![7; 8];
        convert(&frame(0, 0, 16, 128, 128), ColorMatrix::Auto, &mut out);
        assert!(out.is_empty());
    }
}
