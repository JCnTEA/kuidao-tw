use std::{
    ffi::OsStr,
    fs::{File, OpenOptions},
    mem::size_of,
    os::windows::{
        ffi::OsStrExt,
        fs::OpenOptionsExt,
        io::AsRawHandle,
    },
    path::Path,
    process::{Command, Stdio},
};
use windows::{
    core::{Owned, PCWSTR, PWSTR},
    Win32::{
        Foundation::{HANDLE, HWND, INVALID_HANDLE_VALUE},
        Security::{
            Cryptography::{szOID_COMMON_NAME, CertGetNameStringW, CERT_NAME_ATTR_TYPE},
            WinTrust::{
                WTHelperGetProvCertFromChain, WTHelperGetProvSignerFromChain,
                WTHelperProvDataFromStateData, WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2,
                WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO, WTD_CACHE_ONLY_URL_RETRIEVAL,
                WTD_CHOICE_FILE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
            },
            DuplicateTokenEx, GetTokenInformation, SecurityImpersonation, TokenElevation,
            TokenPrimary, TOKEN_ACCESS_MASK, TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE,
            TOKEN_ELEVATION, TOKEN_QUERY,
        },
        Storage::FileSystem::FILE_SHARE_READ,
        System::Threading::{
            CreateProcessWithTokenW, GetCurrentProcess, OpenProcess, OpenProcessToken,
            CREATE_PROCESS_LOGON_FLAGS, PROCESS_CREATION_FLAGS, PROCESS_INFORMATION,
            PROCESS_QUERY_LIMITED_INFORMATION, STARTUPINFOW,
        },
    },
};

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

fn expected_publisher(filename: &str, signer: &str) -> bool {
    if filename.eq_ignore_ascii_case("Feishu.exe") {
        signer.eq_ignore_ascii_case("Beijing Feishu Technology Co.,Ltd.")
            || signer.eq_ignore_ascii_case("Beijing Feishu Technology Co., Ltd.")
    } else {
        filename.eq_ignore_ascii_case("Lark.exe")
            && signer.eq_ignore_ascii_case("Lark Technologies Pte. Ltd.")
    }
}

unsafe fn signer_matches(state: HANDLE, expected: impl FnOnce(&str) -> bool) -> bool {
    let provider = WTHelperProvDataFromStateData(state);
    if provider.is_null() {
        return false;
    }
    let signer = WTHelperGetProvSignerFromChain(provider, 0, false, 0);
    if signer.is_null() {
        return false;
    }
    let Some(cert) = WTHelperGetProvCertFromChain(signer, 0).as_ref() else {
        return false;
    };
    if cert.pCert.is_null() {
        return false;
    }
    let mut name = [0u16; 128];
    let count = CertGetNameStringW(
        cert.pCert,
        CERT_NAME_ATTR_TYPE,
        0,
        Some(szOID_COMMON_NAME.as_ptr().cast()),
        Some(&mut name),
    ) as usize;
    count > 1 && count <= name.len()
        && String::from_utf16(&name[..count - 1]).is_ok_and(|name| expected(&name))
}

unsafe fn trusted_file(path: PCWSTR, file: HANDLE, expected: impl FnOnce(&str) -> bool) -> bool {
    let mut info = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: path,
        hFile: file,
        ..Default::default()
    };
    let mut trust = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut info },
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL,
        ..Default::default()
    };
    let mut policy = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let no_window = HWND(INVALID_HANDLE_VALUE.0);
    let trusted = WinVerifyTrust(no_window, &mut policy, (&mut trust as *mut WINTRUST_DATA).cast()) == 0
        && signer_matches(trust.hWVTStateData, expected);
    trust.dwStateAction = WTD_STATEACTION_CLOSE;
    let _ = WinVerifyTrust(no_window, &mut policy, (&mut trust as *mut WINTRUST_DATA).cast());
    trusted
}

pub(crate) fn lock_signed_executable(path: &Path, expected: impl FnOnce(&str) -> bool) -> Option<File> {
    let wide_path = wide(path.as_os_str());
    let file = OpenOptions::new().read(true).share_mode(FILE_SHARE_READ.0).open(path).ok()?;
    // 啟動結束前持有這個句柄，簽名通過的文件不能被替換。
    unsafe { trusted_file(PCWSTR(wide_path.as_ptr()), HANDLE(file.as_raw_handle()), expected) }.then_some(file)
}

unsafe fn process_token(process: HANDLE, rights: TOKEN_ACCESS_MASK) -> windows::core::Result<Owned<HANDLE>> {
    let mut token = HANDLE::default();
    OpenProcessToken(process, rights, &mut token)?;
    Ok(Owned::new(token))
}

unsafe fn elevated(token: HANDLE) -> windows::core::Result<bool> {
    let mut elevation = TOKEN_ELEVATION::default();
    let mut size = 0;
    GetTokenInformation(token, TokenElevation, Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
        size_of::<TOKEN_ELEVATION>() as u32, &mut size)?;
    Ok(elevation.TokenIsElevated != 0)
}

pub(super) fn launch_feishu(path: &Path, pid: u32) -> Result<(), &'static str> {
    let path = std::fs::canonicalize(path).map_err(|_| "無法確認客戶端程序的實際路徑")?;
    let dirname = path.parent().ok_or("無法確認客戶端安裝目錄")?;
    let filename = path.file_name().and_then(OsStr::to_str).ok_or("無法確認客戶端程序名")?;
    let wide_path = wide(path.as_os_str());
    // 在創建子進程前持有文件，避免文件在簽名檢查和啟動之間被替換。
    let file = OpenOptions::new().read(true).share_mode(FILE_SHARE_READ.0)
        .open(&path).map_err(|_| "無法讀取客戶端程序進行簽名驗證")?;
    unsafe {
        if !trusted_file(PCWSTR(wide_path.as_ptr()), HANDLE(file.as_raw_handle()), |signer| expected_publisher(filename, signer)) {
            return Err("客戶端簽名或簽名發佈者無法驗證");
        }
        let current = process_token(GetCurrentProcess(), TOKEN_QUERY).map_err(|_| "無法確認島仔運行權限")?;
        if !elevated(*current).map_err(|_| "無法確認島仔運行權限")? {
            return Command::new(&path).current_dir(dirname)
                .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
                .spawn().map(|_| ()).map_err(|_| "啟動客戶端程序失敗");
        }
        // 使用已匹配的客戶端令牌，管理員運行的島仔也不以自身權限啟動飛書。
        let process = Owned::new(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
            .map_err(|_| "無法獲取客戶端運行權限")?);
        let token = process_token(*process, TOKEN_QUERY | TOKEN_DUPLICATE)
            .map_err(|_| "無法獲取客戶端運行權限")?;
        if elevated(*token).map_err(|_| "無法確認客戶端運行權限")? {
            return Err("客戶端以管理員身份運行，請從托盤手動打開");
        }
        let mut primary = HANDLE::default();
        DuplicateTokenEx(*token, TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_ASSIGN_PRIMARY, None,
            SecurityImpersonation, TokenPrimary, &mut primary).map_err(|_| "無法獲取客戶端普通權限令牌")?;
        let primary = Owned::new(primary);
        let wide_dir = wide(dirname.as_os_str());
        let startup = STARTUPINFOW { cb: size_of::<STARTUPINFOW>() as u32, ..Default::default() };
        let mut child = PROCESS_INFORMATION::default();
        CreateProcessWithTokenW(*primary, CREATE_PROCESS_LOGON_FLAGS(0), PCWSTR(wide_path.as_ptr()),
            PWSTR::null(), PROCESS_CREATION_FLAGS(0), None, PCWSTR(wide_dir.as_ptr()), &startup, &mut child)
            .map_err(|_| "無法以普通權限打開客戶端，請從托盤手動打開")?;
        let _process = Owned::new(child.hProcess);
        let _thread = Owned::new(child.hThread);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feishu_launch_requires_the_exact_signing_publisher() {
        assert!(expected_publisher("Feishu.exe", "Beijing Feishu Technology Co.,Ltd."));
        assert!(expected_publisher("Lark.exe", "LARK TECHNOLOGIES PTE. LTD."));
        assert!(!expected_publisher("Feishu.exe", "Beijing Feishu Technology Co.,Ltd. Test"));
        assert!(!expected_publisher("Feishu.exe", "Lark Technologies Pte. Ltd."));
        assert!(!expected_publisher("other.exe", "Beijing Feishu Technology Co.,Ltd."));

        let file = std::env::current_exe().unwrap();
        let wide_file = wide(file.as_os_str());
        let handle = OpenOptions::new().read(true).open(&file).unwrap();
        assert!(!unsafe { trusted_file(PCWSTR(wide_file.as_ptr()), HANDLE(handle.as_raw_handle()), |signer| expected_publisher("Feishu.exe", signer)) });
    }
}
