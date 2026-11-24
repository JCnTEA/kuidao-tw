//! 用 Windows 的音訊迴路（loopback）直接聽「電腦現在在播什麼」。
//!
//! 跟麥克風相比，迴路拿到的是系統正在輸出的聲音本身：沒有環境噪音、不用開喇叭音量，
//! 也不會收到旁人說話。只有在真的有聲音輸出時才會累積樣本。

#[cfg(windows)]
pub fn capture_mono(seconds: u32) -> Option<(Vec<f32>, u32)> {
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator,
        AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK,
        WAVEFORMATEX,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    if seconds == 0 {
        return None;
    }

    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None).ok()?;
        let format_ptr: *mut WAVEFORMATEX = client.GetMixFormat().ok()?;
        let format = *format_ptr;
        let init = client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK,
            0,
            0,
            format_ptr,
            None,
        );
        CoTaskMemFree(Some(format_ptr as _));
        init.ok()?;

        let channels = format.nChannels.max(1) as usize;
        let rate = format.nSamplesPerSec;
        // WAVE_FORMAT_IEEE_FLOAT = 0x0003、WAVE_FORMAT_EXTENSIBLE = 0xFFFE；windows crate 沒匯出前兩個常數。
        const IEEE_FLOAT: u16 = 0x0003;
        const PCM: u16 = 0x0001;
        const EXTENSIBLE: u16 = 0xFFFE;
        let is_float = format.wFormatTag == IEEE_FLOAT
            || (format.wFormatTag == EXTENSIBLE && format.wBitsPerSample == 32);
        let is_pcm16 = format.wFormatTag == PCM && format.wBitsPerSample == 16;
        if !is_float && !is_pcm16 {
            return None;
        }

        let capture: IAudioCaptureClient = client.GetService().ok()?;
        client.Start().ok()?;

        let want = (seconds as usize).saturating_mul(rate as usize);
        let mut mono: Vec<f32> = Vec::with_capacity(want);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds as u64 + 4);
        let mut saw_audio = false;

        while mono.len() < want && std::time::Instant::now() < deadline {
            let mut packet = capture.GetNextPacketSize().unwrap_or(0);
            if packet == 0 {
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            }
            while packet > 0 {
                let mut data: *mut u8 = std::ptr::null_mut();
                let mut frames: u32 = 0;
                let mut flags: u32 = 0;
                if capture
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
                    .is_err()
                {
                    break;
                }
                let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;
                if !silent && !data.is_null() && frames > 0 {
                    if is_float {
                        let samples = std::slice::from_raw_parts(data as *const f32, frames as usize * channels);
                        for frame in samples.chunks_exact(channels) {
                            let sum: f32 = frame.iter().sum();
                            mono.push(sum / channels as f32);
                        }
                    } else {
                        let samples = std::slice::from_raw_parts(data as *const i16, frames as usize * channels);
                        for frame in samples.chunks_exact(channels) {
                            let sum: f32 = frame.iter().map(|value| *value as f32 / 32768.0).sum();
                            mono.push(sum / channels as f32);
                        }
                    }
                    saw_audio = true;
                }
                let _ = capture.ReleaseBuffer(frames);
                packet = capture.GetNextPacketSize().unwrap_or(0);
            }
        }
        let _ = client.Stop();

        if !saw_audio || mono.len() < (rate as usize / 2) {
            return None;
        }
        Some((mono, rate))
    }
}

#[cfg(not(windows))]
pub fn capture_mono(_seconds: u32) -> Option<(Vec<f32>, u32)> {
    None
}
