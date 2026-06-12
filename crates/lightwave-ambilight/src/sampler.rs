use std::str::FromStr;

use crate::capture::Frame;

/// Pixels are sampled every this many rows/columns; box colors are heavy
/// averages, so denser sampling adds cost without changing the result.
const SAMPLE_STEP: usize = 4;

/// Floor on the averaging weight so colorless scenes still average
/// normally instead of dividing by ~zero and flickering.
const WEIGHT_FLOOR: f32 = 0.01;

/// Screen edge the LED strip mirrors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Bottom,
    Top,
    Left,
    Right,
}

impl FromStr for Edge {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "bottom" => Ok(Self::Bottom),
            "top" => Ok(Self::Top),
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            _ => Err(format!(
                "unknown edge {s:?}; expected bottom, top, left or right"
            )),
        }
    }
}

impl std::fmt::Display for Edge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Bottom => "bottom",
            Self::Top => "top",
            Self::Left => "left",
            Self::Right => "right",
        })
    }
}

/// Reduces a frame's edge band to per-box average colors.
///
/// Averaging happens in linear light (screen pixels are sRGB-encoded;
/// averaging the encoded values biases dark), and each pixel is weighted
/// by its chroma so vivid content isn't drowned out by large grey or
/// near-grey areas — the main cause of washed-out ambilight.
pub struct Sampler {
    boxes: usize,
    edge: Edge,
    depth: f32,
    vividness: f32,
    reverse: bool,
    /// sRGB byte -> linear-light value.
    to_linear: [f32; 256],
}

impl Sampler {
    pub fn new(boxes: usize, edge: Edge, depth: f32, vividness: f32, reverse: bool) -> Self {
        let mut to_linear = [0.0; 256];
        for (i, v) in to_linear.iter_mut().enumerate() {
            *v = srgb_to_linear(i as f32 / 255.0);
        }

        Self {
            boxes,
            edge,
            depth,
            vividness,
            reverse,
            to_linear,
        }
    }

    /// Average the frame's edge band into per-box colors (sRGB, 0..=1).
    /// Returns an empty vec if the frame doesn't match its own geometry.
    pub fn sample(&self, frame: &Frame<'_>) -> Vec<[f32; 3]> {
        let bpp = frame.format.bytes_per_pixel();
        let (ro, go, bo) = frame.format.rgb_offsets();

        if frame.width == 0
            || frame.height == 0
            || frame.data.len() < (frame.height - 1) * frame.stride + frame.width * bpp
        {
            return Vec::new();
        }

        // Boxes split the screen along the strip's edge; `depth` controls
        // how far the sampled band reaches inward from that edge.
        let (along_len, band_len) = match self.edge {
            Edge::Bottom | Edge::Top => (frame.width, frame.height),
            Edge::Left | Edge::Right => (frame.height, frame.width),
        };
        let band = ((band_len as f32 * self.depth) as usize).clamp(1, band_len);
        let band_range = match self.edge {
            Edge::Top | Edge::Left => 0..band,
            Edge::Bottom | Edge::Right => band_len - band..band_len,
        };

        let mut out = Vec::with_capacity(self.boxes);
        for i in 0..self.boxes {
            let a0 = i * along_len / self.boxes;
            let a1 = ((i + 1) * along_len / self.boxes).max(a0 + 1);

            let mut sum = [0.0f32; 3];
            let mut weight_sum = 0.0f32;
            for along in (a0..a1).step_by(SAMPLE_STEP) {
                for across in band_range.clone().step_by(SAMPLE_STEP) {
                    let (x, y) = match self.edge {
                        Edge::Bottom | Edge::Top => (along, across),
                        Edge::Left | Edge::Right => (across, along),
                    };

                    let px = y * frame.stride + x * bpp;
                    let r = self.to_linear[frame.data[px + ro] as usize];
                    let g = self.to_linear[frame.data[px + go] as usize];
                    let b = self.to_linear[frame.data[px + bo] as usize];

                    let chroma = r.max(g).max(b) - r.min(g).min(b);
                    let weight = WEIGHT_FLOOR + self.vividness * chroma * chroma;

                    sum[0] += r * weight;
                    sum[1] += g * weight;
                    sum[2] += b * weight;
                    weight_sum += weight;
                }
            }

            out.push([
                linear_to_srgb(sum[0] / weight_sum),
                linear_to_srgb(sum[1] / weight_sum),
                linear_to_srgb(sum[2] / weight_sum),
            ]);
        }

        if self.reverse {
            out.reverse();
        }

        out
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::PixelFormat;

    fn rgbx(width: usize, height: usize, pixel: impl Fn(usize, usize) -> [u8; 3]) -> Vec<u8> {
        let mut data = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            for x in 0..width {
                let [r, g, b] = pixel(x, y);
                data.extend_from_slice(&[r, g, b, 0xff]);
            }
        }
        data
    }

    fn frame(width: usize, height: usize, data: &[u8]) -> Frame<'_> {
        Frame {
            width,
            height,
            stride: width * 4,
            format: PixelFormat::Rgbx,
            data,
        }
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.01,
            "expected ~{expected}, got {actual}"
        );
    }

    #[test]
    fn solid_color_passes_through() {
        let data = rgbx(32, 32, |_, _| [200, 40, 90]);
        let sampler = Sampler::new(4, Edge::Bottom, 0.25, 1.0, false);

        let boxes = sampler.sample(&frame(32, 32, &data));

        assert_eq!(boxes.len(), 4);
        for color in boxes {
            assert_close(color[0], 200.0 / 255.0);
            assert_close(color[1], 40.0 / 255.0);
            assert_close(color[2], 90.0 / 255.0);
        }
    }

    #[test]
    fn vividness_zero_averages_in_linear_light() {
        // Half black, half white: the linear mean is 0.5, which encodes
        // to ~0.735 sRGB. A gamma-space mean would give 0.5 instead.
        let data = rgbx(32, 32, |x, _| if x < 16 { [0; 3] } else { [255; 3] });
        let sampler = Sampler::new(1, Edge::Bottom, 1.0, 0.0, false);

        let boxes = sampler.sample(&frame(32, 32, &data));

        assert_close(boxes[0][0], 0.735);
        assert_close(boxes[0][1], 0.735);
        assert_close(boxes[0][2], 0.735);
    }

    #[test]
    fn vivid_pixels_outweigh_grey() {
        let data = rgbx(32, 32, |x, _| if x < 16 { [128; 3] } else { [255, 0, 0] });
        let plain = Sampler::new(1, Edge::Bottom, 1.0, 0.0, false);
        let vivid = Sampler::new(1, Edge::Bottom, 1.0, 4.0, false);

        let plain_box = plain.sample(&frame(32, 32, &data))[0];
        let vivid_box = vivid.sample(&frame(32, 32, &data))[0];

        // Weighting should pull the average toward pure red.
        assert!(vivid_box[0] > plain_box[0]);
        assert!(vivid_box[1] < plain_box[1]);
        assert!(vivid_box[0] > 0.95);
        assert!(vivid_box[1] < 0.2);
    }

    #[test]
    fn band_samples_the_requested_edge() {
        let data = rgbx(16, 16, |_, y| if y < 8 { [0, 0, 255] } else { [255, 0, 0] });

        let bottom = Sampler::new(1, Edge::Bottom, 0.5, 0.0, false);
        let top = Sampler::new(1, Edge::Top, 0.5, 0.0, false);

        let bottom_box = bottom.sample(&frame(16, 16, &data))[0];
        let top_box = top.sample(&frame(16, 16, &data))[0];

        assert_close(bottom_box[0], 1.0);
        assert_close(bottom_box[2], 0.0);
        assert_close(top_box[0], 0.0);
        assert_close(top_box[2], 1.0);
    }

    #[test]
    fn boxes_follow_the_edge_and_reverse_flips_them() {
        let data = rgbx(16, 16, |x, _| if x < 8 { [0, 255, 0] } else { [255, 0, 0] });

        let forward = Sampler::new(2, Edge::Bottom, 1.0, 0.0, false);
        let reversed = Sampler::new(2, Edge::Bottom, 1.0, 0.0, true);

        let f = forward.sample(&frame(16, 16, &data));
        let r = reversed.sample(&frame(16, 16, &data));

        assert_close(f[0][1], 1.0); // left box is green
        assert_close(f[1][0], 1.0); // right box is red
        assert_close(r[0][0], 1.0);
        assert_close(r[1][1], 1.0);
    }

    #[test]
    fn vertical_edges_split_boxes_by_row() {
        let data = rgbx(16, 16, |_, y| if y < 8 { [0, 255, 0] } else { [255, 0, 0] });
        let sampler = Sampler::new(2, Edge::Left, 1.0, 0.0, false);

        let boxes = sampler.sample(&frame(16, 16, &data));

        assert_close(boxes[0][1], 1.0); // top box is green
        assert_close(boxes[1][0], 1.0); // bottom box is red
    }

    #[test]
    fn malformed_frame_yields_nothing() {
        let data = rgbx(8, 4, |_, _| [255; 3]);
        let sampler = Sampler::new(2, Edge::Bottom, 0.5, 1.0, false);

        // Claim a bigger frame than the buffer holds.
        let boxes = sampler.sample(&frame(8, 8, &data));

        assert!(boxes.is_empty());
    }
}
