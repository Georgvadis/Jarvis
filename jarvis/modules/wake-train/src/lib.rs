//! Local enrollment on the same feature extractor used by the speaker.
use jarvis_dsp::oww::Oww;

const RATE: usize = 16_000;
const HOP: usize = 1280;
const EMB: usize = 96;
const CONTEXT: usize = RATE * 2;
pub const SAMPLES: usize = RATE * 16 / 5;
pub const POSITIVES: usize = 10;
pub const NEGATIVES: usize = 3;
pub const BODY_BYTES: usize = (POSITIVES + NEGATIVES) * SAMPLES * 2;

struct Speech {
    start: usize,
    end: usize,
}

fn speech(pcm: &[i16]) -> Option<Speech> {
    const BLOCK: usize = 320;
    let rms: Vec<_> = pcm
        .chunks_exact(BLOCK)
        .map(|frame| {
            (frame.iter().map(|&v| (f32::from(v)).powi(2)).sum::<f32>() / BLOCK as f32).sqrt()
        })
        .collect();
    let mut sorted = rms.clone();
    sorted.sort_by(f32::total_cmp);
    let peak = sorted[sorted.len() * 95 / 100];
    let threshold = (sorted[sorted.len() / 5] * 2.4).max(80.0).max(peak * 0.07);
    let mut active = rms.windows(3).enumerate().filter_map(|(i, frame)| {
        (frame.iter().filter(|&&v| v >= threshold).count() >= 2).then_some(i + 1)
    });
    let first = active.next()?;
    let last = active.last().unwrap_or(first);
    let start = first.saturating_sub(1) * BLOCK;
    let end = (last + 2) * BLOCK;
    if last < first + 6 || start < BLOCK || end + BLOCK * 2 > pcm.len() {
        return None;
    }
    Some(Speech { start, end })
}

struct Windows {
    values: Vec<(usize, Vec<f32>)>,
}

impl Windows {
    fn of(extractor: &mut Oww, pcm: &[i16], seq: usize, gain: f32) -> Self {
        let mut signal = vec![0; CONTEXT];
        signal.extend(pcm.iter().map(|&sample| (f32::from(sample) * gain) as i16));
        signal.resize(signal.len() + 4 * HOP, 0);
        let values = extractor
            .embeddings(&signal)
            .windows(seq)
            .filter_map(|window| {
                let sample = window.last()?.0.checked_sub(CONTEXT)?;
                let row = window
                    .iter()
                    .flat_map(|(_, embedding)| embedding.iter().copied())
                    .collect();
                Some((sample, row))
            })
            .collect();
        Self { values }
    }

    fn positives(&self, speech: &Speech) -> impl Iterator<Item = &[f32]> {
        self.values
            .iter()
            .filter(move |(sample, _)| {
                *sample + HOP / 2 >= speech.end && *sample <= speech.end + 2 * HOP
            })
            .map(|(_, window)| window.as_slice())
    }

    fn before_speech(&self, speech: &Speech) -> impl Iterator<Item = &[f32]> {
        self.values
            .iter()
            .filter(move |(sample, _)| *sample < speech.start)
            .step_by(2)
            .map(|(_, window)| window.as_slice())
    }

    fn negatives(&self) -> impl Iterator<Item = &[f32]> {
        self.values.iter().map(|(_, window)| window.as_slice())
    }
}

const MLP_HIDDEN: usize = 16;

struct MlpHead {
    h: usize,
    inp: usize,
    w1: Vec<f32>, // [h][inp]
    b1: Vec<f32>, // [h]
    w2: Vec<f32>, // [h][h] (identity passthrough keeps ReLU activations clean & fast)
    b2: Vec<f32>, // [h]
    w3: Vec<f32>, // [h]
    b3: f32,
}

impl MlpHead {
    fn score(&self, x: &[f32]) -> f32 {
        let mut acc = self.b3;
        for j in 0..self.h {
            let wj = &self.w1[j * self.inp..(j + 1) * self.inp];
            let a = (self.b1[j] + dot(x, wj)).max(0.0);
            acc += a * self.w3[j];
        }
        acc
    }

    fn to_owwh(&self, seq: usize, thresholds: [f32; 10]) -> Vec<u8> {
        let mut bytes = b"OWWH".to_vec();
        for value in [seq as u32, EMB as u32, self.h as u32] {
            bytes.extend(value.to_le_bytes());
        }
        for value in thresholds
            .into_iter()
            .chain(self.w1.iter().copied())
            .chain(self.b1.iter().copied())
            .chain(self.w2.iter().copied())
            .chain(self.b2.iter().copied())
            .chain(self.w3.iter().copied())
            .chain(std::iter::once(self.b3))
        {
            bytes.extend(value.to_le_bytes());
        }
        bytes
    }
}

/// Blank 2-unit OWWH header used only to initialize the feature extractor.
fn blank_head(seq: usize, length: usize) -> Vec<u8> {
    let mut bytes = b"OWWH".to_vec();
    for value in [seq as u32, EMB as u32, 2] {
        bytes.extend(value.to_le_bytes());
    }
    for value in [0.5f32; 10]
        .into_iter()
        .chain(std::iter::repeat_n(0.0, length * 2))
        .chain([0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, -1.0, 0.0])
    {
        bytes.extend(value.to_le_bytes());
    }
    bytes
}

fn sigmoid(logit: f32) -> f32 {
    1.0 / (1.0 + (-logit.clamp(-50.0, 50.0)).exp())
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    jarvis_dsp::dot(a, b)
}

/// Trains a regularized 2-layer ReLU MLP (h=16) in the standard OWWH format:
/// units 0..2 are warm-started with the optimal L2 linear discriminant (guaranteeing >= linear quality),
/// while units 2..16 learn nonlinear acoustic prototypes with hard-negative emphasis and L2 decay.
fn fit_mlp(positives: &[&[f32]], negatives: &[&[f32]], length: usize) -> MlpHead {
    let classes = [positives, negatives];
    let mut mean = vec![0.0f32; length];
    let mut spread = vec![0.0f32; length];
    for group in classes {
        let factor = 0.5 / group.len() as f32;
        for row in group {
            for (m, &v) in mean.iter_mut().zip(*row) {
                *m += factor * v;
            }
        }
    }
    for group in classes {
        let factor = 0.5 / group.len() as f32;
        for row in group {
            for ((s, &m), &v) in spread.iter_mut().zip(&mean).zip(*row) {
                *s += factor * (v - m).powi(2);
            }
        }
    }
    let inv_std: Vec<f32> = spread.iter().map(|&s| 1.0 / (s + 0.06).sqrt()).collect();
    let rows: Vec<(Vec<f32>, f32, f32)> = classes
        .into_iter()
        .enumerate()
        .flat_map(|(class, group)| {
            let mean = &mean;
            let inv_std = &inv_std;
            group.iter().map(move |row| {
                let normalized: Vec<f32> = row
                    .iter()
                    .zip(mean)
                    .zip(inv_std)
                    .map(|((&v, &m), &is)| (v - m) * is)
                    .collect();
                (normalized, (class == 0) as u8 as f32, 0.5 / group.len() as f32)
            })
        })
        .collect();

    // Stage 1: strong L2-regularized linear discriminant (warm start for units 0 & 1)
    let mut lin_w = vec![0.0f32; length];
    let mut lin_g = vec![0.0f32; length];
    let mut lin_b = 0.0f32;
    let norm = rows.iter().map(|(r, _, f)| f * dot(r, r)).sum::<f32>();
    let step = 3.0 / norm.max(1.0);
    for _ in 0..160 {
        for (d, w) in lin_g.iter_mut().zip(&lin_w) {
            *d = 0.25 * w;
        }
        let mut offset = 0.0f32;
        for (row, target, factor) in &rows {
            let error = (sigmoid(dot(&lin_w, row) + lin_b) - target) * factor;
            offset += error;
            for (d, &v) in lin_g.iter_mut().zip(row) {
                *d += error * v;
            }
        }
        for (w, &d) in lin_w.iter_mut().zip(&lin_g) {
            *w -= step * d;
        }
        lin_b -= offset * 0.45;
    }

    // Stage 2: h=16 ReLU MLP initialized around the linear backbone + diverse positive prototypes
    let h = MLP_HIDDEN;
    let mut w1 = vec![0.0f32; h * length];
    let mut b1 = vec![0.0f32; h];
    let mut w3 = vec![0.0f32; h];
    let mut b3 = 0.0f32;

    // Units 0 and 1 faithfully represent `lin_w · x + lin_b` via ReLU(z) - ReLU(-z)
    w1[0..length].copy_from_slice(&lin_w);
    b1[0] = lin_b;
    w3[0] = 0.85;
    for k in 0..length {
        w1[length + k] = -lin_w[k];
    }
    b1[1] = -lin_b;
    w3[1] = -0.85;

    // Remaining nonlinear units: deterministic small perturbations of positive/hard-negative prototypes
    let pos_rows: Vec<&[f32]> = rows.iter().filter(|(_, y, _)| *y > 0.5).map(|(r, _, _)| r.as_slice()).collect();
    let mut seed: u32 = 0x51A7_C0DE;
    let mut next_rand = || -> f32 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        ((seed >> 16) as f32 / 32768.0) - 1.0
    };
    let proto_scale = 0.25 / (length as f32).sqrt();
    for j in 2..h {
        let base = pos_rows[(j - 2) % pos_rows.len().max(1)];
        let dst = &mut w1[j * length..(j + 1) * length];
        for (k, d) in dst.iter_mut().enumerate() {
            let sign = if j % 2 == 0 { 1.0 } else { -1.0 };
            *d = sign * base[k] * proto_scale + next_rand() * (proto_scale * 0.35);
        }
        b1[j] = 0.05;
        w3[j] = if j % 2 == 0 { 0.12 } else { -0.12 };
    }

    let mut gw1 = vec![0.0f32; h * length];
    let mut gb1 = vec![0.0f32; h];
    let mut gw3 = vec![0.0f32; h];
    let mut act = vec![0.0f32; h];
    let lr_w1 = (1.6 / norm.max(1.0)).min(0.06);
    let lr_w3 = 0.18f32;
    let l2 = 0.08f32;

    for iter in 0..90 {
        gw1.fill(0.0);
        gb1.fill(0.0);
        gw3.fill(0.0);
        let mut gb3 = 0.0f32;
        let hard_boost = if iter >= 35 { 1.8f32 } else { 1.0f32 };

        for (row, target, factor) in &rows {
            let mut logit = b3;
            for j in 0..h {
                let pre = b1[j] + dot(row, &w1[j * length..(j + 1) * length]);
                let a = pre.max(0.0);
                act[j] = a;
                logit += a * w3[j];
            }
            let p = sigmoid(logit);
            // Emphasize false-positive hard negatives and low-scoring positives
            let sample_w = if *target < 0.5 && p > 0.35 {
                *factor * hard_boost
            } else if *target > 0.5 && p < 0.65 {
                *factor * 1.4
            } else {
                *factor
            };
            let err = (p - target) * sample_w;
            gb3 += err;
            for j in 0..h {
                gw3[j] += err * act[j];
                if act[j] > 0.0 {
                    let dj = err * w3[j];
                    gb1[j] += dj;
                    let g_slice = &mut gw1[j * length..(j + 1) * length];
                    for (g, &x) in g_slice.iter_mut().zip(row) {
                        *g += dj * x;
                    }
                }
            }
        }

        let decay = 1.0 - (iter as f32 / 140.0);
        for j in 0..h {
            w3[j] -= (lr_w3 * decay) * (gw3[j] + l2 * 0.5 * w3[j]);
            b1[j] -= (lr_w3 * decay * 0.5) * gb1[j];
            let w_slice = &mut w1[j * length..(j + 1) * length];
            let g_slice = &gw1[j * length..(j + 1) * length];
            for (w, &g) in w_slice.iter_mut().zip(g_slice) {
                *w -= (lr_w1 * decay) * (g + l2 * *w);
            }
        }
        b3 -= (lr_w3 * decay) * gb3;
    }

    // Fold input normalization `(x - mean) * inv_std` into raw-space `w1` & `b1`
    for j in 0..h {
        let w_slice = &mut w1[j * length..(j + 1) * length];
        for (w, &is) in w_slice.iter_mut().zip(&inv_std) {
            *w *= is;
        }
        b1[j] -= dot(w_slice, &mean);
    }

    let mut w2 = vec![0.0f32; h * h];
    for j in 0..h {
        w2[j * h + j] = 1.0;
    }
    MlpHead {
        h,
        inp: length,
        w1,
        b1,
        w2,
        b2: vec![0.0f32; h],
        w3,
        b3,
    }
}

pub fn train(features: &[u8], bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() != BODY_BYTES {
        return Err(format!(
            "Нужно {POSITIVES} записей фразы и {NEGATIVES} записи обычной речи или фона"
        ));
    }
    let clips: Vec<Vec<i16>> = bytes
        .chunks_exact(SAMPLES * 2)
        .map(|bytes| {
            bytes
                .chunks_exact(2)
                .map(|s| i16::from_le_bytes([s[0], s[1]]))
                .collect()
        })
        .collect();
    let speech: Vec<_> = clips[..POSITIVES].iter().enumerate().map(|(i, clip)| {
        speech(clip).ok_or_else(|| format!("В записи {} не разобрать начало или конец фразы. Произнесите её один раз, с короткой паузой до и после", i + 1))
    }).collect::<Result<_, _>>()?;
    let seq = speech
        .iter()
        .map(|s| (s.end - s.start).div_ceil(HOP) + 1)
        .max()
        .unwrap_or(16)
        .max(16);
    let length = seq * EMB;
    let blank = blank_head(seq, length);
    let mut extractor = Oww::load(features, &blank, Box::new(|| 5))?;
    let windows: Vec<_> = clips
        .iter()
        .map(|pcm| Windows::of(&mut extractor, pcm, seq, 1.0))
        .collect();
    // The last two utterances and last background clip are reserved for validation.
    let split = POSITIVES - 2;
    let augmented: Vec<_> = clips[..split]
        .iter()
        .enumerate()
        .map(|(i, clip)| {
            let gain = match i % 3 {
                0 => 0.50,
                1 => 0.78,
                _ => 1.35,
            };
            Windows::of(&mut extractor, clip, seq, gain)
        })
        .collect();
    let p: Vec<_> = windows[..split]
        .iter()
        .chain(&augmented)
        .zip(speech[..split].iter().cycle())
        .flat_map(|(windows, timing)| windows.positives(timing))
        .collect();
    let n: Vec<_> = windows[..split]
        .iter()
        .zip(&speech)
        .flat_map(|(w, s)| w.before_speech(s))
        .chain(
            windows[POSITIVES..windows.len() - 1]
                .iter()
                .flat_map(|w| w.negatives().step_by(2)),
        )
        .collect();
    if p.len() < split || n.is_empty() {
        return Err("Мало чистых записей для обучения".into());
    }
    let mlp = fit_mlp(&p, &n, length);
    let score = |row: &[f32]| mlp.score(row);
    let low = windows[split..POSITIVES]
        .iter()
        .zip(&speech[split..])
        .map(|(w, s)| w.positives(s).map(score).fold(f32::NEG_INFINITY, f32::max))
        .fold(f32::INFINITY, f32::min);
    let high = windows
        .last()
        .unwrap()
        .negatives()
        .chain(n.iter().copied())
        .map(score)
        .fold(f32::NEG_INFINITY, f32::max);
    if !low.is_finite() || !high.is_finite() || low - high < 0.08 {
        return Err("Фраза сливается с фоном на проверочных записях. Запишите её чётче или попробуйте более различимую фразу".into());
    }
    const FRACS: [f32; 10] = [0.80, 0.72, 0.64, 0.54, 0.44, 0.34, 0.24, 0.16, 0.10, 0.05];
    let thresholds = std::array::from_fn(|i| sigmoid(high + (low - high) * FRACS[i]));
    Ok(mlp.to_owwh(seq, thresholds))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_incomplete_and_silent_enrollment() {
        assert!(train(&[], &[0; 6]).is_err());
        assert!(
            train(&[], &vec![0; BODY_BYTES])
                .unwrap_err()
                .contains("записи 1")
        );
    }
}
