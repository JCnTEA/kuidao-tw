//! Shazam 相容的音訊指紋：把 16 kHz 單聲道 PCM 轉成頻譜峰值，再編成 Shazam 認得的二進位簽章。
//!
//! 演算法與封包格式改寫自 MIT 授權的 `middledot/shazam.py`（Rust 核心），
//! 只保留需要的部分：不依賴檔案解碼器、不需要 PyO3，輸入直接是我們自己抓到的樣本。

use byteorder::{LittleEndian, WriteBytesExt};
use chfft::RFft1D;
use std::io::{Cursor, Seek, SeekFrom, Write};

/// 取樣率固定 16 kHz，對應 Shazam 封包裡 sample rate id = 3。
pub const SAMPLE_RATE: u32 = 16_000;

/// 一次辨識送出的音訊長度（秒）。
pub const SEGMENT_SECONDS: u32 = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Band {
    /// 250–520 Hz
    Low = 0,
    /// 520–1450 Hz
    Mid = 1,
    /// 1450–3500 Hz
    High = 2,
    /// 3500–5500 Hz
    Top = 3,
}

#[derive(Clone, Copy, Debug)]
pub struct Peak {
    pub fft_pass: u32,
    pub magnitude: u16,
    pub frequency_bin: u16,
}

#[derive(Clone, Debug, Default)]
pub struct Signature {
    pub sample_rate_hz: u32,
    pub number_samples: u32,
    pub bands: Vec<(Band, Vec<Peak>)>,
}

/// 漢寧窗：`sin²(πi/(N-1))`。舊版直接抄了 GPL 專案的表，這裡改成即時算，數值只差千分之二。
fn hanning_window(len: usize) -> Vec<f32> {
    (0..len)
        .map(|i| {
            let angle = std::f32::consts::PI * i as f32 / (len as f32 - 1.0);
            angle.sin().powi(2)
        })
        .collect()
}

/// 把單聲道 16 kHz 樣本做成簽章；超過 12 秒時取中段，跟 Shazam 官方客戶端一致。
pub fn make_signature(pcm: &[i16]) -> Signature {
    let window = hanning_window(2048);
    let segment = segment_of(pcm);
    let mut signature = Signature {
        sample_rate_hz: SAMPLE_RATE,
        number_samples: segment.len() as u32,
        bands: Vec::new(),
    };
    let mut ring = [0i16; 2048];
    let mut ring_index = 0usize;
    let mut reordered = vec![0f32; 2048];
    let mut fft_outputs = vec![vec![0f32; 1025]; 256];
    let mut fft_index = 0usize;
    let mut spread = vec![vec![0f32; 1025]; 256];
    let mut spread_index = 0usize;
    let mut spread_done: u32 = 0;
    let mut fft = RFft1D::<f32>::new(2048);

    for chunk in segment.chunks_exact(128) {
        ring[ring_index..ring_index + 128].copy_from_slice(chunk);
        ring_index = (ring_index + 128) & 2047;

        for i in 0..2048 {
            reordered[i] = ring[(i + ring_index) & 2047] as f32 * window[i];
        }

        let complex = fft.forward(&reordered);
        {
            let out = &mut fft_outputs[fft_index];
            for i in 0..=1024 {
                out[i] = ((complex[i].re * complex[i].re + complex[i].im * complex[i].im) / 131_072.0)
                    .max(0.0000000001);
            }
        }
        fft_index = (fft_index + 1) & 255;

        let real = &fft_outputs[(fft_index + 255) & 255];
        let spread_now = &mut spread[spread_index];
        spread_now.copy_from_slice(real);
        for i in 0..=1022 {
            spread_now[i] = spread_now[i].max(spread_now[i + 1]).max(spread_now[i + 2]);
        }
        let snapshot = spread_now.clone();
        for position in 0..=1024 {
            for former in [1usize, 3, 6] {
                let target = &mut spread[(spread_index + 256 - former) & 255];
                target[position] = target[position].max(snapshot[position]);
            }
        }
        spread_index = (spread_index + 1) & 255;
        spread_done += 1;

        if spread_done < 46 {
            continue;
        }

        let fft_minus_46 = &fft_outputs[(fft_index + 256 - 46) & 255];
        let spread_minus_49 = &spread[(spread_index + 256 - 49) & 255];

        for bin in 10..=1014 {
            if fft_minus_46[bin] < 1.0 / 64.0 || fft_minus_46[bin] < spread_minus_49[bin - 1] {
                continue;
            }
            let mut neighbour = 0.0f32;
            for offset in [-10i32, -7, -4, -3, 1, 2, 5, 8] {
                neighbour = neighbour.max(spread_minus_49[(bin as i32 + offset) as usize]);
            }
            if fft_minus_46[bin] <= neighbour {
                continue;
            }
            let mut other = neighbour;
            for offset in [-53i32, -45, 165, 172, 179, 186, 193, 200, 214, 221, 228, 235, 242, 249] {
                let table = &spread[(spread_index as i32 + offset + 256) as usize & 255];
                other = other.max(table[bin - 1]);
            }
            if fft_minus_46[bin] <= other {
                continue;
            }

            let pass = spread_done - 46;
            let magnitude = |v: f32| v.ln().max(1.0 / 64.0) * 1477.3 + 6144.0;
            let centre = magnitude(fft_minus_46[bin]);
            let before = magnitude(fft_minus_46[bin - 1]);
            let after = magnitude(fft_minus_46[bin + 1]);
            let variation = centre * 2.0 - before - after;
            if variation < 0.0 {
                continue;
            }
            let sub_bin = (after - before) * 32.0 / variation;
            let corrected = (bin as u16) * 64 + sub_bin as u16;
            let hz = corrected as f32 * (SAMPLE_RATE as f32 / 2.0 / 1024.0 / 64.0);
            let band = match hz as i32 {
                250..=519 => Band::Low,
                520..=1449 => Band::Mid,
                1450..=3499 => Band::High,
                3500..=5500 => Band::Top,
                _ => continue,
            };
            let entry = match signature.bands.iter_mut().find(|(b, _)| *b == band) {
                Some((_, peaks)) => peaks,
                None => {
                    signature.bands.push((band, Vec::new()));
                    &mut signature.bands.last_mut().expect("剛加入").1
                }
            };
            entry.push(Peak { fft_pass: pass, magnitude: centre as u16, frequency_bin: corrected });
        }
    }

    signature
}

fn segment_of(pcm: &[i16]) -> &[i16] {
    let limit = (SEGMENT_SECONDS * SAMPLE_RATE) as usize;
    if pcm.len() <= limit {
        return pcm;
    }
    let middle = pcm.len() / 2;
    let half = limit / 2;
    &pcm[middle - half..middle + half]
}

/// 依 Shazam 二進位格式編碼：48 位元組標頭 + 各頻段的型別長度值序列。
pub fn encode(signature: &Signature) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::<u8>::new());
    let _ = cursor.write_u32::<LittleEndian>(0xcafe_2580);
    let _ = cursor.write_u32::<LittleEndian>(0);
    let _ = cursor.write_u32::<LittleEndian>(0);
    let _ = cursor.write_u32::<LittleEndian>(0x9411_9c00);
    for _ in 0..3 {
        let _ = cursor.write_u32::<LittleEndian>(0);
    }
    let sample_rate_id: u32 = match signature.sample_rate_hz {
        8000 => 1,
        11025 => 2,
        16000 => 3,
        32000 => 4,
        44100 => 5,
        48000 => 6,
        _ => 3,
    };
    let _ = cursor.write_u32::<LittleEndian>(sample_rate_id << 27);
    for _ in 0..2 {
        let _ = cursor.write_u32::<LittleEndian>(0);
    }
    let samples_plus = signature.number_samples + (signature.sample_rate_hz as f32 * 0.24) as u32;
    let _ = cursor.write_u32::<LittleEndian>(samples_plus);
    let _ = cursor.write_u32::<LittleEndian>((15 << 19) + 0x40000);
    let _ = cursor.write_u32::<LittleEndian>(0x4000_0000);
    let _ = cursor.write_u32::<LittleEndian>(0);

    let mut bands = signature.bands.clone();
    bands.sort_by_key(|(band, _)| *band);
    for (band, peaks) in bands {
        let mut peaks_cursor = Cursor::new(Vec::<u8>::new());
        let mut last_pass = 0u32;
        for peak in peaks {
            if peak.fft_pass < last_pass {
                continue;
            }
            if peak.fft_pass - last_pass >= 255 {
                let _ = peaks_cursor.write_u8(0xff);
                let _ = peaks_cursor.write_u32::<LittleEndian>(peak.fft_pass);
                last_pass = peak.fft_pass;
            }
            let _ = peaks_cursor.write_u8((peak.fft_pass - last_pass) as u8);
            let _ = peaks_cursor.write_u16::<LittleEndian>(peak.magnitude);
            let _ = peaks_cursor.write_u16::<LittleEndian>(peak.frequency_bin);
            last_pass = peak.fft_pass;
        }
        let peaks_bytes = peaks_cursor.into_inner();
        let _ = cursor.write_u32::<LittleEndian>(0x6003_0040 + band as u32);
        let _ = cursor.write_u32::<LittleEndian>(peaks_bytes.len() as u32);
        let _ = cursor.write(&peaks_bytes);
        for _ in 0..((4 - peaks_bytes.len() as u32 % 4) % 4) {
            let _ = cursor.write_u8(0);
        }
    }

    let total = cursor.position() as u32;
    let _ = cursor.seek(SeekFrom::Start(8));
    let _ = cursor.write_u32::<LittleEndian>(total - 48);
    let _ = cursor.seek(SeekFrom::Start(52));
    let _ = cursor.write_u32::<LittleEndian>(total - 48);
    let payload = cursor.get_ref()[8..].to_vec();
    let crc = crc32fast::hash(&payload);
    let _ = cursor.seek(SeekFrom::Start(4));
    let _ = cursor.write_u32::<LittleEndian>(crc);
    cursor.into_inner()
}

/// 把任意取樣率、單聲道 f32 樣本線性轉成 16 kHz i16，供指紋使用。
pub fn resample_to_16k(samples: &[f32], source_rate: u32) -> Vec<i16> {
    if samples.is_empty() || source_rate == 0 {
        return Vec::new();
    }
    if source_rate == SAMPLE_RATE {
        return samples.iter().map(|s| (s * 32767.0).clamp(-32768.0, 32767.0) as i16).collect();
    }
    let ratio = source_rate as f64 / SAMPLE_RATE as f64;
    let out_len = (samples.len() as f64 / ratio) as usize;
    let mut out = Vec::with_capacity(out_len);
    for index in 0..out_len {
        let position = index as f64 * ratio;
        let left = position.floor() as usize;
        let right = (left + 1).min(samples.len() - 1);
        let weight = (position - left as f64) as f32;
        let value = samples[left] * (1.0 - weight) + samples[right] * weight;
        out.push((value * 32767.0).clamp(-32768.0, 32767.0) as i16);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_header_is_shazam_compatible() {
        let pcm = vec![0i16; 16000];
        let signature = make_signature(&pcm);
        let bytes = encode(&signature);
        assert_eq!(&bytes[0..4], &0xcafe_2580u32.to_le_bytes());
        assert_eq!(&bytes[12..16], &0x9411_9c00u32.to_le_bytes());
        assert_eq!(bytes.len() as u32 - 48, u32::from_le_bytes(bytes[8..12].try_into().unwrap()));
        let crc = crc32fast::hash(&bytes[8..]);
        assert_eq!(crc, u32::from_le_bytes(bytes[4..8].try_into().unwrap()));
    }

    #[test]
    fn silence_has_no_peaks_but_keeps_sample_count() {
        let signature = make_signature(&vec![0i16; 16000]);
        assert_eq!(signature.number_samples, 16000);
        assert!(signature.bands.iter().all(|(_, peaks)| peaks.is_empty()));
    }

    #[test]
    fn loud_tone_produces_peaks() {
        let mut pcm = Vec::new();
        for i in 0..(SEGMENT_SECONDS * SAMPLE_RATE) {
            let t = i as f32 / SAMPLE_RATE as f32;
            pcm.push(((2.0 * std::f32::consts::PI * 440.0 * t).sin() * 24000.0) as i16);
        }
        let signature = make_signature(&pcm);
        let peaks: usize = signature.bands.iter().map(|(_, peaks)| peaks.len()).sum();
        assert!(peaks > 0, "持續音應該產生峰值");
    }

    #[test]
    fn resamples_48k_to_16k_length() {
        let samples = vec![0.0f32; 48_000];
        let out = resample_to_16k(&samples, 48_000);
        assert_eq!(out.len(), 16_000);
    }
}
