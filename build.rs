//! 把鬼島图标作为 RT_GROUP_ICON #1 编进 exe，供 LoadImageW(hInst, 1, ...) 取用。
//! 不引入额外依赖：直接调用 Windows SDK 的 rc.exe，把 .res 交给链接器。

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn find_rc() -> Option<PathBuf> {
    let roots = [
        r"C:\Program Files (x86)\Windows Kits\10\bin",
        r"C:\Program Files\Windows Kits\10\bin",
    ];
    let mut found: Vec<PathBuf> = Vec::new();
    for root in roots {
        let Ok(entries) = fs::read_dir(root) else { continue };
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let rc = dir.join("x64").join("rc.exe");
            if rc.exists() {
                found.push(rc);
            }
        }
    }
    found.sort();
    found.pop()
}

fn main() {
    let rc_source = env::var("KUIDAO_RC").unwrap_or_else(|_| "app.rc".into());
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=app.dev.rc");
    println!("cargo:rerun-if-env-changed=KUIDAO_RC");
    println!("cargo:rerun-if-changed=assets/kuidao.ico");
    println!("cargo:rerun-if-changed=assets/kuidao_logo.ico");
    println!("cargo:rerun-if-changed=assets/kuidao_dev.ico");
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("daozai.res");
    let Some(rc) = find_rc() else {
        // 交叉編譯（mingw）沒有 rc.exe：改用 windres，圖示、版本資源與管理員權限清單才能一起編進 exe。
        if let Some(windres) = find_windres() {
            let obj = PathBuf::from(env::var("OUT_DIR").unwrap()).join("kuidao.res");
            let status = Command::new(&windres)
                .arg("-I").arg(".")
                .arg("--codepage=65001")
                .arg(&rc_source).arg("-O").arg("coff").arg("-o").arg(&obj)
                .status();
            if status.map(|s| s.success()).unwrap_or(false) {
                println!("cargo:rustc-link-arg={}", obj.display());
            } else {
                println!("cargo:warning=windres 編譯 app.rc 失敗，圖示與權限清單將缺席");
            }
            return;
        }
        println!("cargo:warning=rc.exe / windres 均未找到，跳过图标资源");
        return;
    };
    let status = Command::new(&rc)
        .arg("/nologo")
        .arg("/fo")
        .arg(&out)
        .arg(&rc_source)
        .status();
    if !status.map(|s| s.success()).unwrap_or(false) {
        println!("cargo:warning=rc.exe 编译 app.rc 失败");
        return;
    }
    println!("cargo:rustc-link-arg={}", out.display());
}

/// mingw 工具鏈的 windres：支援 `WINDRES` 覆寫，預設找 PATH 與常見前綴。
fn find_windres() -> Option<PathBuf> {
    if let Ok(p) = env::var("WINDRES") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    for name in ["x86_64-w64-mingw32-windres", "windres"] {
        let ok = Command::new(name).arg("--version").output().map(|o| o.status.success()).unwrap_or(false);
        if ok {
            return Some(PathBuf::from(name));
        }
    }
    None
}
