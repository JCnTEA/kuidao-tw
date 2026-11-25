#![cfg(windows)]

use daozai::calendar::{self, Config, Provider};
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn failed_configuration_preserves_old_credentials() {
    let _guard = ENV_LOCK.lock().unwrap();
    let id = format!("{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
    let path = std::env::temp_dir().join(format!("daozai-credential-test-{id}"));
    std::fs::write(&path, "不是目錄").unwrap();
    std::env::set_var("KUIDAO_DATA_DIR", &path);
    let old = Config { provider: Provider::Google, username: format!("test-old-{id}"), href: None, server: None };
    let new = Config { username: format!("test-new-{id}"), ..old.clone() };
    calendar::secrets::save(&old, "舊密碼").unwrap();
    assert!(calendar::save_connection(&new, "新密碼", Some(&old)).is_err());
    assert_eq!(calendar::secrets::load(&old).unwrap(), "舊密碼");
    assert!(calendar::secrets::load(&new).is_err());
    assert!(calendar::save_connection(&old, "更新後的密碼", Some(&old)).is_err());
    assert_eq!(calendar::secrets::load(&old).unwrap(), "舊密碼");
    calendar::secrets::delete(&old);
    calendar::secrets::delete(&new);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn calendar_platform_connections_are_independent() {
    let _guard = ENV_LOCK.lock().unwrap();
    let id = format!("platforms-{}", std::process::id());
    let path = std::env::temp_dir().join(format!("daozai-{id}"));
    std::fs::create_dir_all(&path).unwrap();
    std::env::set_var("KUIDAO_DATA_DIR", &path);
    let google = Config { provider: Provider::Google, username: format!("google-{id}"), href: None, server: None };
    let icloud = Config { provider: Provider::ICloud, username: format!("icloud-{id}"), href: None, server: None };
    calendar::save_connection(&google, "Google 測試密碼", None).unwrap();
    // 模擬升級前只儲存一個平台的設定。
    std::fs::write(path.join("calendar.json"), calendar::encode_config(&google)).unwrap();
    calendar::save_connection(&icloud, "iCloud 測試密碼", Some(&google)).unwrap();
    assert_eq!(calendar::secrets::load(&google).unwrap(), "Google 測試密碼");
    assert_eq!(calendar::load_configs().len(), 2);
    assert_eq!(calendar::load_config(), Some(icloud.clone()));
    calendar::save_config(&google).unwrap();
    assert_eq!(calendar::load_config(), Some(google.clone()));
    calendar::select_provider(Provider::Custom).unwrap();
    assert_eq!(calendar::load_config(), None);
    assert_eq!(calendar::load_configs().len(), 2);
    calendar::select_provider(Provider::Google).unwrap();
    calendar::forget_config().unwrap();
    assert_eq!(calendar::load_config(), None);
    assert_eq!(calendar::load_configs(), vec![icloud.clone()]);
    calendar::secrets::delete(&google);
    calendar::secrets::delete(&icloud);
    std::fs::remove_file(path.join("calendar.json")).unwrap();
    std::fs::remove_dir(path).unwrap();
}
