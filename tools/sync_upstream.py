#!/usr/bin/env python3
"""鬼島 · 上游自動同步

把上游 Renly1994/xiaotang-daozai 的最新原始碼拉下來，自動完成：
  1. 繁體化（OpenCC s2twp）＋ 台灣用語替換
  2. 功能字串修復（歌詞來源、飛書標題、驗證碼關鍵詞、天氣省會表）
  3. 品牌改名（島仔→鬼島 / Daozai→Kuidao）
  4. 版本號對齊上游，寫入 app.rc 與 app.manifest
  5. 保留本倉庫自訂檔案（圖示、授權、README-TW、build.rs、workflow）

用法：
    python tools/sync_upstream.py                # 有新版才同步
    python tools/sync_upstream.py --force        # 無條件重跑
    python tools/sync_upstream.py --repo X/Y     # 改上游

退出碼：0 有更新並完成；2 已是相同版本；1 失敗
"""
from __future__ import annotations

import argparse
import io
import re
import shutil
import sys
import tarfile
import tempfile
import urllib.request
import pathlib

from opencc import OpenCC

CC = OpenCC("s2twp")
ROOT = pathlib.Path(__file__).resolve().parent.parent
VERSION_FILE = ROOT / ".upstream-version"

# ── 台灣用語表（詞組優先，長詞先替換）─────────────────────────────
TW_PLAIN = [
    ("藍芽", "藍牙"), ("快捷方式", "捷徑"), ("日程表", "行事曆"), ("日程", "行程"),
    ("日曆", "行事曆"), ("全天", "整日"), ("托盤", "系統匣"), ("皮膚", "主題"),
    ("界面", "介面"), ("刷新", "重新整理"), ("回車", "Enter"), ("用戶", "使用者"),
    ("質量", "品質"), ("優化", "最佳化"), ("調試", "偵錯"), ("加載", "載入"),
    ("重置", "重設"), ("菜單", "選單"), ("截屏", "截圖"), ("字體", "字型"),
    ("帶寬", "頻寬"), ("服務器", "伺服器"), ("光標", "游標"), ("剪貼板", "剪貼簿"),
    ("郵箱", "信箱"), ("壁紙", "桌布"), ("二維碼", "QR Code"), ("鼠標", "滑鼠"),
    ("屏幕", "螢幕"), ("視頻", "影片"), ("音頻", "音訊"), ("網絡", "網路"),
    ("內存", "記憶體"), ("軟件", "軟體"), ("登錄", "登入"), ("默認", "預設"),
    ("打印", "列印"), ("搜索", "搜尋"), ("粘貼", "貼上"), ("剪切", "剪下"),
    ("保存", "儲存"), ("信息", "資訊"), ("設置", "設定"), ("源碼", "原始碼"),
    ("代碼", "程式碼"), ("編程", "程式設計"), ("彈窗", "彈出視窗"),
    ("頁簽", "分頁"), ("賬", "帳"), ("服務端", "伺服器端"),
]
TW_PLAIN.sort(key=lambda x: -len(x[0]))
TW_REGEX = [
    (re.compile(r"應用(?!程式)"), "應用程式"),
    (re.compile(r"文件(?!夾)"), "檔案"),
    (re.compile(r"文件夾"), "資料夾"),
    (re.compile(r"(?<!本地)本地(?!化)"), "本機"),
    (re.compile(r"在線"), "線上"),
]
HAN = re.compile(r"[\u4e00-\u9fff]")


def tw(text: str) -> str:
    s = CC.convert(text)
    for a, b in TW_PLAIN:
        s = s.replace(a, b)
    for rx, b in TW_REGEX:
        s = rx.sub(b, s)
    return s.replace("應用程式程式", "應用程式")


def localize_file(text: str) -> tuple[str, int]:
    """只轉換字串字面量，跳過註解；避免破壞程式碼與鍵值。"""
    out, i, n = [], 0, 0
    while i < len(text):
        j = i
        m = None
        while j < len(text):
            c = text[j]
            if c == "/" and j + 1 < len(text) and text[j + 1] == "/":
                k = text.find("\n", j)
                j = len(text) if k < 0 else k + 1
                continue
            if c == "/" and j + 1 < len(text) and text[j + 1] == "*":
                k = text.find("*/", j + 2)
                j = len(text) if k < 0 else k + 2
                continue
            if c == '"' or (c == "r" and j + 1 < len(text) and text[j + 1] in ('"', "#")):
                m = j
                break
            j += 1
        if m is None:
            out.append(text[i:])
            break
        out.append(text[i:m])
        if text[m] == "r":
            hashes = ""
            k = m + 1
            while k < len(text) and text[k] == "#":
                hashes += "#"
                k += 1
            endtok = '"' + hashes
            e = text.find(endtok, k + 1)
            if e < 0:
                out.append(text[m:])
                break
            body = text[k + 1:e]
            if HAN.search(body) and not body.lstrip().startswith("http"):
                body = tw(body)
                n += 1
            out.append(text[m:k + 1] + body + endtok)
            i = e + len(endtok)
        else:
            k = m + 1
            buf = []
            while k < len(text):
                if text[k] == "\\":
                    buf.append(text[k:k + 2])
                    k += 2
                    continue
                if text[k] == '"':
                    break
                buf.append(text[k])
                k += 1
            body = "".join(buf)
            if HAN.search(body):
                body = tw(body)
                n += 1
            out.append('"' + body + '"')
            i = k + 1
    return "".join(out), n


# ── 功能字串修復（轉換會破壞的比對鍵）───────────────────────────
MAIL_PROVIDER_FIXES: list[tuple[str, str]] = [
    ("pub enum Provider {\n    Qq,", "pub enum Provider {\n    Yahoo,\n    Qq,"),
    ("        Provider::Qq\n    }", "        Provider::Yahoo\n    }"),
    ('            Provider::Qq => "qq",', '            Provider::Yahoo => "yahoo",\n            Provider::Qq => "qq",'),
    ('            Provider::Qq => "QQ 信箱",', '            Provider::Yahoo => "Yahoo 奇摩信箱",\n            Provider::Qq => "QQ 信箱",'),
    ('            Provider::Microsoft => "Outlook / Hotmail（暫不支援）",', '            Provider::Microsoft => "Outlook / Hotmail",'),
    ('            Provider::Qq => "imap.qq.com",', '            Provider::Yahoo => "imap.mail.yahoo.com",\n            Provider::Qq => "imap.qq.com",'),
    ('            Provider::Microsoft => MICROSOFT_UNSUPPORTED,', '            Provider::Microsoft => MICROSOFT_UNSUPPORTED,\n            Provider::Yahoo => "1. 登入 Yahoo 奇摩信箱網頁版，進入「帳號資訊」→「帳號安全」。\\n2. 開啟「允許應用程式使用較不安全的登入方式」，或依帳號版本生成「應用程式密碼」。\\n3. 信箱欄填完整 Yahoo 地址，下方填應用程式密碼，不是登入密碼。",'),
    ('            Provider::Qq => ("開啟 QQ 信箱網頁版", "https://mail.qq.com/"),', '            Provider::Yahoo => ("開啟 Yahoo 帳號安全", "https://login.yahoo.com/account/security"),\n            Provider::Qq => ("開啟 QQ 信箱網頁版", "https://mail.qq.com/"),'),
    ('            "163" => Provider::Mail163,', '            "yahoo" => Provider::Yahoo,\n            "163" => Provider::Mail163,'),
]

MAIL_ALL_OLD = """    pub const ALL: [Provider; 8] = [
        Provider::Qq,
        Provider::Mail163,
        Provider::Mail126,
        Provider::Exmail,
        Provider::ICloud,
        Provider::Gmail,
        Provider::Microsoft,
        Provider::Custom,
    ];"""
MAIL_ALL_NEW = """    /// 台灣版：選單只列本地可連線的服務；大陸信箱仍能解析舊帳號，但不再出現在清單。
    pub const ALL: [Provider; 5] = [
        Provider::Yahoo,
        Provider::ICloud,
        Provider::Gmail,
        Provider::Microsoft,
        Provider::Custom,
    ];"""


def apply_tw_mail_providers() -> None:
    """台灣版信箱清單：移除大陸服務，加入 Yahoo 奇摩，微軟改為可選。"""
    p = ROOT / "src/mail.rs"
    if not p.exists():
        print("   [略過] 信箱清單：找不到 src/mail.rs")
        return
    t = p.read_text(encoding="utf-8")
    if 'Provider::Yahoo => "yahoo"' in t:
        print("   [略過] 信箱清單已是台灣版")
        return
    hits = 0
    for old, new in MAIL_PROVIDER_FIXES:
        if old in t:
            t = t.replace(old, new, 1)
            hits += 1
    if MAIL_ALL_OLD in t:
        t = t.replace(MAIL_ALL_OLD, MAIL_ALL_NEW, 1)
        hits += 1
    else:
        t2 = re.sub(r"pub const ALL: \[Provider; 8\] = \[[^;]*\];", MAIL_ALL_NEW, t, count=1, flags=re.S)
        if t2 != t:
            t = t2
            hits += 1
        else:
            print("   [未命中] 信箱清單（上游可能已改結構，需人工確認）")
    p.write_text(t, encoding="utf-8")
    print(f"   [修復] 信箱清單改為台灣可連線服務（{hits} 項）")


def hide_support_section() -> None:
    """台灣版不顯示贊助／收款碼區塊。

    上游「關於」頁末尾有「支援作者」卡片，台灣版整塊隱藏：
    保留程式碼與授權，僅以常數關閉入口。可重複執行（已處理則跳過）。
    """
    p = ROOT / "src/extras.rs"
    if not p.exists():
        print("   [略過] 支援作者區塊：找不到 src/extras.rs")
        return
    t = p.read_text(encoding="utf-8")
    if "SUPPORT_SECTION_ENABLED" in t:
        print("   [略過] 支援作者區塊已隱藏")
        return
    lines = t.splitlines()
    done = False
    for i, line in enumerate(lines):
        if "settings_card(ui,support_author" not in line:
            continue
        j = i
        while j > 0 and "settings_label(ui," not in lines[j-1]:
            j -= 1
            if i - j > 3:
                break
        indent = line[: len(line) - len(line.lstrip())]
        if "settings_label(ui," in lines[j-1]:
            lines[j-1: i+1] = [
                f"{indent}// 台灣版：不顯示「支援作者」與收款碼區塊",
                f"{indent}if SUPPORT_SECTION_ENABLED{{",
                f"{indent}    settings_label(ui,\"支援作者\");",
                f"{indent}    settings_card(ui,support_author);",
                f"{indent}}}",
            ]
            done = True
        break
    if not done:
        print("   [未命中] 支援作者區塊（上游可能已改結構，需人工確認）")
        return
    t = "\n".join(lines) + "\n"
    # 常數插在第一個 use 區塊之後，避免放在 //! 文件註解前面
    m = re.search(r"^use [^\n]*;$", t, re.M)
    if m:
        t = t[: m.end()] + "\n\n/// 台灣版開關：支援作者（贊助／收款碼）區塊是否顯示。\nconst SUPPORT_SECTION_ENABLED: bool = false;" + t[m.end():]
    p.write_text(t, encoding="utf-8")
    print("   [修復] 支援作者區塊已隱藏")


FIXUPS: list[tuple[str, str, str, str]] = [
    # (檔案 glob, 說明, 舊, 新)
    ("src/lyrics.rs", "歌詞汽水音樂來源", '"汽水音樂"', '"汽水音乐"'),
    ("src/smtc.rs", "媒體來源比對", '"汽水音樂"', '"汽水音乐"'),
    ("src/messenger/desktop.rs", "飛書視窗標題", '"飛書"', '"飞书"'),
]

BRAND = [("島仔", "鬼島"), ("岛仔", "鬼島"), ("Daozai", "Kuidao"), ("DAOZAI", "KUIDAO")]


def brand(text: str) -> str:
    for a, b in BRAND:
        text = text.replace(a, b)
    return text


def download(repo: str) -> tuple[pathlib.Path, str]:
    url = f"https://codeload.github.com/{repo}/tar.gz/refs/heads/main"
    print(f"→ 下載 {url}")
    with urllib.request.urlopen(url, timeout=120) as r:
        data = r.read()
    tmp = pathlib.Path(tempfile.mkdtemp(prefix="upstream-"))
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as tf:
        tf.extractall(tmp)
    roots = [p for p in tmp.iterdir() if p.is_dir()]
    src_root = roots[0]
    cargo = (src_root / "Cargo.toml").read_text(encoding="utf-8")
    m = re.search(r'^version\s*=\s*"([^"]+)"', cargo, re.M)
    version = m.group(1) if m else "0.0.0"
    return src_root, version


KEEP_PATHS = [
    "app.manifest", "app.rc", "build.rs", "NOTICE", "README-TW.md",
    "授權產生器.py", "tools", ".github", "docs/TW-TERMS.md",
    "vendor", "assets/kuidao.ico", "assets/kuidao_logo.ico", "assets/kuidao_dev.ico",
    "assets/kuidao_banner.png", "assets/kuidao-logo.png", "assets/kuidao_mascot.webp",
    "assets/pet-species",
    "docs/index.html", "hist", "installer/setup.nsi",
]


def sync(src_root: pathlib.Path, version: str) -> int:
    keep_dir = pathlib.Path(tempfile.mkdtemp(prefix="keep-"))
    for rel in KEEP_PATHS:
        p = ROOT / rel
        if p.exists():
            dst = keep_dir / rel
            dst.parent.mkdir(parents=True, exist_ok=True)
            if p.is_dir():
                shutil.copytree(p, dst)
            else:
                shutil.copy2(p, dst)

    # 清掉舊的上游檔案（src、vendor、Cargo.*、installer 等）
    for name in ("src", "vendor", "Cargo.toml", "Cargo.lock", "installer", "assets", "docs",
                 "README.md", "LICENSE", "package.ps1", "check-runtime.ps1"):
        p = ROOT / name
        if p.is_dir():
            shutil.rmtree(p)
        elif p.exists():
            p.unlink()
    shutil.copytree(src_root, ROOT, dirs_exist_ok=True, ignore=shutil.ignore_patterns(".git"))

    for rel in KEEP_PATHS:
        p = keep_dir / rel
        if p.exists():
            dst = ROOT / rel
            dst.parent.mkdir(parents=True, exist_ok=True)
            if p.is_dir():
                shutil.copytree(p, dst, dirs_exist_ok=True)
            else:
                shutil.copy2(p, dst)

    # 台灣化
    total = 0
    for p in (ROOT / "src").rglob("*.rs"):
        t = p.read_text(encoding="utf-8")
        new, c = localize_file(t)
        if new != t:
            p.write_text(new, encoding="utf-8")
            total += c
    print(f"→ 繁中化字串 {total} 條")

    # 功能修復
    hits = 0
    for rel, why, old, new in FIXUPS:
        p = ROOT / rel
        if not p.exists():
            print(f"   [略過] {why}：找不到 {rel}")
            continue
        t = p.read_text(encoding="utf-8")
        if old in t:
            p.write_text(t.replace(old, new), encoding="utf-8")
            hits += 1
            print(f"   [修復] {why}")
        else:
            print(f"   [未命中] {why}（上游可能已改結構，需人工確認）")

    # 天氣省會表：補上簡體「台湾」鍵，值改台灣慣用「台北」
    wp = ROOT / "src/weather.rs"
    if wp.exists():
        t = wp.read_text(encoding="utf-8")
        if '("臺灣","臺北")' in t:
            t = t.replace('("臺灣","臺北")', '("臺灣","台北"),("台湾","台北")')
            t = t.replace("PROVINCE_CAPITALS: [(&str, &str); 34]", "PROVINCE_CAPITALS: [(&str, &str); 35]")
            wp.write_text(t, encoding="utf-8")
            print("   [修復] 天氣省會（臺灣/台湾 → 台北）")
        elif '("臺灣","台北")' in t:
            print("   [略過] 天氣省會已是台灣用語")
        else:
            print("   [未命中] 天氣省會表結構已變，需人工確認")

    # 台灣版：隱藏「支援作者」贊助區塊
    hide_support_section()

    # 台灣版：信箱服務清單在地化
    apply_tw_mail_providers()

    # 台灣版：行政區後綴補繁體，讓「士林區」「內湖區」查得到
    wp2 = ROOT / "src/weather.rs"
    if wp2.exists():
        t = wp2.read_text(encoding="utf-8")
        old = "const ADMIN_SUFFIXES: [char; 4] = ['市', '县', '区', '省'];"
        if old in t:
            t = t.replace(old, "/// 繁簡兩套後綴都要能拆：台灣使用者輸入「士林區」「臺北縣」時必須命中。\nconst ADMIN_SUFFIXES: [char; 6] = ['市', '縣', '區', '省', '县', '区'];", 1)
            wp2.write_text(t, encoding="utf-8")
            print("   [修復] 行政區後綴補繁體（區／縣）")
        elif "['市', '縣', '區', '省'" in t:
            print("   [略過] 行政區後綴已是繁體")
        else:
            print("   [未命中] 行政區後綴（上游可能已改結構，需人工確認）")

    # 品牌
    for p in list((ROOT / "src").rglob("*.rs")) + [ROOT / "Cargo.toml", ROOT / "app.rc", ROOT / "installer/setup.nsi"]:
        if not p.exists():
            continue
        t = p.read_text(encoding="utf-8")
        nb = brand(t)
        if nb != t:
            p.write_text(nb, encoding="utf-8")

    # 執行檔名與圖示路徑（小寫 daozai 不會被上面的品牌替換處理到）
    for p in (ROOT / "src").rglob("*.rs"):
        t = p.read_text(encoding="utf-8")
        nt = (t.replace('"daozai.exe"', '"kuidao.exe"')
               .replace("assets/daozai.ico", "assets/kuidao.ico")
               .replace("assets/daozai-logo.png", "assets/kuidao-logo.png"))
        if nt != t:
            p.write_text(nt, encoding="utf-8")
    ico = ROOT / "assets/daozai.ico"
    if ico.exists() and not (ROOT / "assets/kuidao.ico").exists():
        shutil.copy2(ico, ROOT / "assets/kuidao.ico")

    # 版本對齊：Cargo 與 app.rc / manifest
    cargo_p = ROOT / "Cargo.toml"
    cargo = cargo_p.read_text(encoding="utf-8")
    cargo = re.sub(r'^version\s*=\s*"[^"]+"', f'version = "{version}"', cargo, count=1, flags=re.M)
    cargo = re.sub(r'^name\s*=\s*"[^"]+"', 'name = "kuidao"', cargo, count=1, flags=re.M)
    cargo = cargo.replace('[[bin]]\nname = "daozai"', '[[bin]]\nname = "kuidao"')
    cargo = re.sub(r'default-run\s*=\s*"[^"]+"', 'default-run = "kuidao"', cargo)
    cargo_p.write_text(cargo, encoding="utf-8")

    rc_p = ROOT / "app.rc"
    if rc_p.exists():
        rc = rc_p.read_text(encoding="utf-8")
        parts = version.split(".")
        while len(parts) < 4:
            parts.append("0")
        v4 = ",".join(parts[:4])
        rc = re.sub(r"FILEVERSION [\d,]+", f"FILEVERSION {v4}", rc)
        rc = re.sub(r"PRODUCTVERSION [\d,]+", f"PRODUCTVERSION {v4}", rc)
        rc = re.sub(r'VALUE "ProductVersion", "[^"]*"', f'VALUE "ProductVersion", "{version}"', rc)
        rc = re.sub(r'VALUE "FileVersion", "[^"]*"', f'VALUE "FileVersion", "{version}"', rc)
        rc_p.write_text(rc, encoding="utf-8")

    man_p = ROOT / "app.manifest"
    if man_p.exists():
        man = man_p.read_text(encoding="utf-8")
        man = re.sub(r'version="[\d.]+"', f'version="{version}.0"', man, count=1)
        man_p.write_text(man, encoding="utf-8")

    # 安裝器版本
    nsi = ROOT / "installer/setup.nsi"
    if nsi.exists():
        t = nsi.read_text(encoding="utf-8")
        t = re.sub(r'VIProductVersion "[^"]*"', f'VIProductVersion "{version}.0"', t)
        t = re.sub(r'!define VERSION "[^"]*"', f'!define VERSION "{version}"', t)
        nsi.write_text(t, encoding="utf-8")

    VERSION_FILE.write_text(version + "\n", encoding="utf-8")
    print(f"\n✔ 同步完成：上游 {version} → 鬼島（修復 {hits} 項）")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default="Renly1994/xiaotang-daozai")
    ap.add_argument("--force", action="store_true")
    a = ap.parse_args()

    src_root, version = download(a.repo)
    old = VERSION_FILE.read_text(encoding="utf-8").strip() if VERSION_FILE.exists() else ""
    if old == version and not a.force:
        print(f"→ 已是最新（{version}），略過")
        return 2
    print(f"→ 上游版本 {version}（本地 {old or '無'}）")
    return sync(src_root, version)


if __name__ == "__main__":
    sys.exit(main())
