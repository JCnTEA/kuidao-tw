//! 用 Windows 的音訊介面聽「電腦現在在播什麼」。
//!
//! 兩條路徑，先試乾淨的、再退到麥克風：
//! 1. 系統輸出迴路（loopback）：拿到的是播放中的聲音本身，沒有環境噪音，不必開喇叭。
//! 2. 內建麥克風：loopback 不可用（沒有輸出裝置、裝置被獨佔、驅動不支援）時改走這條。
//!    Windows 的隱私設定若封鎖「桌面應用程式存取麥克風」，初始化會回 ACCESS_DENIED，
//!    呼叫端要據此跳視窗請使用者開權限，而不是靜靜什麼都不做。

/// 抓不到音訊的原因；呼叫端用它決定要不要請使用者開權限。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureError {
    /// 系統封鎖了桌面應用程式的麥克風存取（Windows 隱私設定）。
    Denied,
    /// 找不到可用的播放或錄音裝置。
    NoDevice,
    /// 有裝置也開得起來，但這段時間沒有任何聲音（沒有播放、或整段靜音）。
    NoSound,
    /// 裝置的取樣格式不是 float32／PCM16，目前無法解析。
    Unsupported,
}

impl CaptureError {
    pub fn label(self) -> &'static str {
        match self {
            Self::Denied => "麥克風權限未開啟",
            Self::NoDevice => "找不到音訊裝置",
            Self::NoSound => "沒有偵測到聲音",
            Self::Unsupported => "音訊格式不支援",
        }
    }
}

#[cfg(windows)]
pub fn capture_mono(seconds: u32) -> Result<(Vec<f32>, u32), CaptureError> {
    if seconds == 0 {
        return Err(CaptureError::NoSound);
    }
    // 先聽系統輸出：最準、也不受麥克風權限影響。
    match capture_endpoint(true, seconds) {
        Ok(samples) => return Ok(samples),
        Err(CaptureError::Denied | CaptureError::NoDevice | CaptureError::Unsupported) => {}
        Err(CaptureError::NoSound) => return Err(CaptureError::NoSound),
    }
    // 系統迴路不行時退到內建麥克風；這裡失敗多半就是權限沒開。
    capture_endpoint(false, seconds)
}

#[cfg(windows)]
fn capture_endpoint(loopback: bool, seconds: u32) -> Result<(Vec<f32>, u32), CaptureError> {
    use windows::core::HRESULT;
    use windows::Win32::Foundation::E_ACCESSDENIED;
    use windows::Win32::Media::Audio::{
        eCapture, eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator,
        MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_LOOPBACK, WAVEFORMATEX,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    // WAVE_FORMAT_IEEE_FLOAT = 0x0003、WAVE_FORMAT_PCM = 0x0001、WAVE_FORMAT_EXTENSIBLE = 0xFFFE；
    // windows crate 沒匯出前兩個常數。
    const IEEE_FLOAT: u16 = 0x0003;
    const PCM: u16 = 0x0001;
    const EXTENSIBLE: u16 = 0xFFFE;

    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|_| CaptureError::NoDevice)?;
        let role = if loopback { eRender } else { eCapture };
        let device = enumerator
            .GetDefaultAudioEndpoint(role, eConsole)
            .map_err(|_| CaptureError::NoDevice)?;
        let client: IAudioClient = device
            .Activate(CLSCTX_ALL, None)
            .map_err(|_| CaptureError::NoDevice)?;
        let format_ptr: *mut WAVEFORMATEX = client.GetMixFormat().map_err(|_| CaptureError::NoDevice)?;
        let format = *format_ptr;
        let flags = if loopback { AUDCLNT_STREAMFLAGS_LOOPBACK } else { 0 };
        let init = client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 0, 0, format_ptr, None);
        CoTaskMemFree(Some(format_ptr as _));
        if let Err(error) = init {
            // Windows 隱私設定封鎖桌面應用程式用麥克風時就是這一個錯誤碼。
            if error.code() == E_ACCESSDENIED || HRESULT(0x8007_0005_u32 as _) == error.code() {
                return Err(CaptureError::Denied);
            }
            return Err(CaptureError::NoDevice);
        }

        let channels = format.nChannels.max(1) as usize;
        let rate = format.nSamplesPerSec;
        if rate == 0 {
            return Err(CaptureError::Unsupported);
        }
        let is_float = format.wFormatTag == IEEE_FLOAT
            || (format.wFormatTag == EXTENSIBLE && format.wBitsPerSample == 32);
        let is_pcm16 = format.wFormatTag == PCM && format.wBitsPerSample == 16;
        if !is_float && !is_pcm16 {
            return Err(CaptureError::Unsupported);
        }

        let capture: IAudioCaptureClient = client.GetService().map_err(|_| CaptureError::NoDevice)?;
        client.Start().map_err(|_| CaptureError::NoDevice)?;

        let want = (seconds as usize).saturating_mul(rate as usize);
        let mut mono: Vec<f32> = Vec::with_capacity(want);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds as u64 + 4);
        let mut saw_audio = false;
        // 沒有任何封包（例如系統根本沒在播、或裝置被獨佔）時不要空等到逾時。
        let mut idle_started = std::time::Instant::now();

        while mono.len() < want && std::time::Instant::now() < deadline {
            let mut packet = capture.GetNextPacketSize().unwrap_or(0);
            if packet == 0 {
                if idle_started.elapsed() > std::time::Duration::from_secs(seconds as u64 + 3) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            }
            idle_started = std::time::Instant::now();
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
                        let samples =
                            std::slice::from_raw_parts(data as *const f32, frames as usize * channels);
                        for frame in samples.chunks_exact(channels) {
                            let sum: f32 = frame.iter().sum();
                            mono.push(sum / channels as f32);
                        }
                    } else {
                        let samples =
                            std::slice::from_raw_parts(data as *const i16, frames as usize * channels);
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
            return Err(CaptureError::NoSound);
        }
        Ok((mono, rate))
    }
}

#[cfg(not(windows))]
pub fn capture_mono(_seconds: u32) -> Result<(Vec<f32>, u32), CaptureError> {
    Err(CaptureError::NoDevice)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_short_and_distinct() {
        let all = [
            CaptureError::Denied,
            CaptureError::NoDevice,
            CaptureError::NoSound,
            CaptureError::Unsupported,
        ];
        for (index, error) in all.iter().enumerate() {
            assert!(!error.label().is_empty());
            assert!(all[index + 1..].iter().all(|other| other.label() != error.label()));
        }
    }
}
