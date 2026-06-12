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
    gamma: f32,
    min_saturation: f32,
    reverse: bool,
    /// sRGB byte -> linear-light value.
    to_linear: [f32; 256],
}

impl Sampler {
    pub fn new(
        boxes: usize,
        edge: Edge,
        depth: f32,
        vividness: f32,
        gamma: f32,
        min_saturation: f32,
        reverse: bool,
    ) -> Self {
        let mut to_linear = [0.0; 256];
        for (i, v) in to_linear.iter_mut().enumerate() {
            *v = srgb_to_linear(i as f32 / 255.0);
        }

        Self {
            boxes,
            edge,
            depth,
            vividness,
            gamma,
            min_saturation,
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

            out.push(self.finish([
                linear_to_srgb(sum[0] / weight_sum),
                linear_to_srgb(sum[1] / weight_sum),
                linear_to_srgb(sum[2] / weight_sum),
            ]));
        }

        if self.reverse {
            out.reverse();
        }

        out
    }

    /// Final per-box adjustments, in sRGB space: lift saturation to the
    /// configured floor, then gamma-correct brightness for the strip.
    fn finish(&self, [r, g, b]: [f32; 3]) -> [f32; 3] {
        let mut color = [r, g, b];
        let v = r.max(g).max(b);
        let chroma = v - r.min(g).min(b);

        // Saturation floor: pull the lower channels away from grey while
        // keeping the max channel and the channel ratios (the hue) fixed.
        // It can only amplify a tint that's already there — pure grey has
        // no hue to recover and is left alone.
        if chroma > 0.0 && chroma < self.min_saturation * v {
            let k = self.min_saturation * v / chroma;
            for c in &mut color {
                *c = v + (*c - v) * k;
            }
        }

        // Gamma on brightness only: scaling every channel by v^(γ-1)
        // darkens as much as per-channel gamma would, but keeps channel
        // ratios — and so hue — intact (per-channel gamma turns orange
        // into red). The strip's response to dark values is far brighter
        // than the monitor's steep sRGB curve; this restores the match.
        if v > 0.0 {
            let scale = v.powf(self.gamma - 1.0);
            for c in &mut color {
                *c *= scale;
            }
        }

        color
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
        let sampler = Sampler::new(4, Edge::Bottom, 0.25, 1.0, 1.0, 0.0, false);

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
        let sampler = Sampler::new(1, Edge::Bottom, 1.0, 0.0, 1.0, 0.0, false);

        let boxes = sampler.sample(&frame(32, 32, &data));

        assert_close(boxes[0][0], 0.735);
        assert_close(boxes[0][1], 0.735);
        assert_close(boxes[0][2], 0.735);
    }

    #[test]
    fn vivid_pixels_outweigh_grey() {
        let data = rgbx(32, 32, |x, _| if x < 16 { [128; 3] } else { [255, 0, 0] });
        let plain = Sampler::new(1, Edge::Bottom, 1.0, 0.0, 1.0, 0.0, false);
        let vivid = Sampler::new(1, Edge::Bottom, 1.0, 4.0, 1.0, 0.0, false);

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

        let bottom = Sampler::new(1, Edge::Bottom, 0.5, 0.0, 1.0, 0.0, false);
        let top = Sampler::new(1, Edge::Top, 0.5, 0.0, 1.0, 0.0, false);

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

        let forward = Sampler::new(2, Edge::Bottom, 1.0, 0.0, 1.0, 0.0, false);
        let reversed = Sampler::new(2, Edge::Bottom, 1.0, 0.0, 1.0, 0.0, true);

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
        let sampler = Sampler::new(2, Edge::Left, 1.0, 0.0, 1.0, 0.0, false);

        let boxes = sampler.sample(&frame(16, 16, &data));

        assert_close(boxes[0][1], 1.0); // top box is green
        assert_close(boxes[1][0], 1.0); // bottom box is red
    }

    #[test]
    fn gamma_expands_darks_for_linear_leds() {
        // Catppuccin-crust-like dark blue-grey: dim on screen, but sent
        // raw to a linear-PWM strip it glows a bright pale white.
        let data = rgbx(16, 16, |_, _| [35, 38, 52]);
        let raw = Sampler::new(1, Edge::Bottom, 1.0, 0.0, 1.0, 0.0, false);
        let corrected = Sampler::new(1, Edge::Bottom, 1.0, 0.0, 2.2, 0.0, false);

        let raw_box = raw.sample(&frame(16, 16, &data))[0];
        let corrected_box = corrected.sample(&frame(16, 16, &data))[0];

        assert_close(raw_box[2], 52.0 / 255.0);
        assert_close(corrected_box[2], (52.0f32 / 255.0).powf(2.2));
        // The whole color should land far darker than the raw encoding.
        for channel in 0..3 {
            assert!(corrected_box[channel] < raw_box[channel] * 0.35);
        }

        // Full-brightness channels are unaffected by gamma.
        let bright = rgbx(16, 16, |_, _| [255, 0, 0]);
        let bright_box = corrected.sample(&frame(16, 16, &bright))[0];
        assert_close(bright_box[0], 1.0);
    }

    #[test]
    fn gamma_preserves_hue() {
        // Orange must stay orange: per-channel gamma would crush the
        // green channel relative to red and shift it toward pure red.
        let data = rgbx(16, 16, |_, _| [255, 165, 0]);
        let sampler = Sampler::new(1, Edge::Bottom, 1.0, 0.0, 2.2, 0.0, false);

        let color = sampler.sample(&frame(16, 16, &data))[0];

        // Max channel at full brightness means gamma changes nothing.
        assert_close(color[0], 1.0);
        assert_close(color[1], 165.0 / 255.0);
        assert_close(color[2], 0.0);
    }

    #[test]
    fn saturation_floor_amplifies_existing_tint() {
        // Crust-like dark blue-grey: saturation (v - min) / v ≈ 0.33.
        let data = rgbx(16, 16, |_, _| [35, 38, 52]);
        let sampler = Sampler::new(1, Edge::Bottom, 1.0, 0.0, 1.0, 0.5, false);

        let color = sampler.sample(&frame(16, 16, &data))[0];

        let v: f32 = 52.0 / 255.0;
        // Max channel is untouched; the others are pulled away from grey
        // until saturation reaches the floor.
        assert_close(color[2], v);
        let saturation = (v - color[0].min(color[1])) / v;
        assert_close(saturation, 0.5);
    }

    #[test]
    fn saturation_floor_leaves_grey_and_vivid_colors_alone() {
        let sampler = Sampler::new(1, Edge::Bottom, 1.0, 0.0, 1.0, 0.5, false);

        // Pure grey has no hue to amplify.
        let grey = rgbx(16, 16, |_, _| [128, 128, 128]);
        let grey_box = sampler.sample(&frame(16, 16, &grey))[0];
        for channel in grey_box {
            assert_close(channel, 128.0 / 255.0);
        }

        // Already above the floor: untouched.
        let red = rgbx(16, 16, |_, _| [255, 0, 0]);
        let red_box = sampler.sample(&frame(16, 16, &red))[0];
        assert_close(red_box[0], 1.0);
        assert_close(red_box[1], 0.0);
    }

    #[test]
    fn malformed_frame_yields_nothing() {
        let data = rgbx(8, 4, |_, _| [255; 3]);
        let sampler = Sampler::new(2, Edge::Bottom, 0.5, 1.0, 1.0, 0.0, false);

        // Claim a bigger frame than the buffer holds.
        let boxes = sampler.sample(&frame(8, 8, &data));

        assert!(boxes.is_empty());
    }
}
