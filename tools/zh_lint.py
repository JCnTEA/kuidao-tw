#!/usr/bin/env python3
"""鬼島中文自檢：只改使用者看得到的字，不動程式註解與識別字。

- Rust：只處理雙引號字串（含 r#"..."#），避免把註解改成半繁半簡。
- HTML／Markdown／NSIS／Workflow：整份文字檢查。
- 自動修：台灣用語、常見重複助詞、重複標點。
- 只報告：需要在上下文判斷的句型，留給人看。
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCAN_DIRS = ("src", "docs", "hist", "installer", ".github/workflows")
SCAN_FILES = ("README.md",)
EXTS = {".rs", ".html", ".md", ".nsi", ".yml", ".yaml"}
SKIP_DIRS = {"vendor", "target", ".git", "assets", "lyrics", "tools", "node_modules"}
SKIP_FILES = {"docs/TW-TERMS.md"}

# 長的先換，避免「用戶名」先被「用戶」拆掉。
TERMS: tuple[tuple[str, str], ...] = (
    ("用戶名稱", "使用者名稱"),
    ("用戶名", "使用者名稱"),
    ("用戶", "使用者"),
    ("賬號", "帳號"),
    ("賬戶", "帳戶"),
    ("設置", "設定"),
    ("默認", "預設"),
    ("加載", "載入"),
    ("保存", "儲存"),
    ("信息", "訊息"),
    ("創建", "建立"),
    ("鏈接", "連結"),
    ("網絡", "網路"),
    ("屏幕", "螢幕"),
    ("鼠標", "滑鼠"),
    ("打印", "列印"),
    ("菜單", "選單"),
    ("視頻", "影片"),
    ("音頻", "音訊"),
    ("錄像", "錄影"),
    ("郵箱", "信箱"),
    ("文件夾", "資料夾"),
    ("後臺", "後台"),
    ("註銷", "登出"),
    ("服務器", "伺服器"),
    ("軟件", "軟體"),
    ("硬件", "硬體"),
    ("內存", "記憶體"),
    ("硬盤", "硬碟"),
    ("帶寬", "頻寬"),
    ("反饋", "回饋"),
    ("支持", "支援"),
    ("下拉框", "下拉選單"),
    ("文本框", "文字方塊"),
    ("復選框", "核取方塊"),
    ("勾選框", "核取方塊"),
    ("單選框", "選項按鈕"),
    ("彈窗", "彈出視窗"),
    ("選中", "選取"),
    ("分辨率", "解析度"),
)

# 只修重複字，不會碰到「看看」「想想」這類正常的疊字。
GRAMMAR: tuple[tuple[re.Pattern[str], str], ...] = tuple(
    (re.compile(bad), good)
    for bad, good in (
        (r"([的了是在個將與及和])\1", r"\1"),
        (r"目前現在", "目前"),
        (r"現在目前", "目前"),
        (r"總共共", "總共"),
        (r"而且且", "而且"),
        (r"但是是", "但是"),
        (r"因為為", "因為"),
        (r"所以以", "所以"),
        (r"如果果", "如果"),
        (r"雖然然", "雖然"),
        (r"可以以", "可以"),
        (r"需要要", "需要"),
        (r"已經經", "已經"),
        (r"繼續續", "繼續"),
        (r"然後後", "然後"),
        (r"並且且", "並且"),
        (r"或是是", "或是"),
        (r"還是是", "還是"),
        (r"只是是", "只是"),
        (r"就是是", "就是"),
        (r"原因是因為", "原因是"),
        (r"大約左右", "大約"),
    )
)

PUNCT = re.compile(r"(，){2,}|(。){2,}|(！){2,}|(？){2,}|(、){2,}")

# 只列出來，不自動改：需要看前後文才知道怎麼修。
REVIEW = (
    re.compile(r"進行[^。！？\n]{0,20}的動作"),
    re.compile(r"通過[^。！？\n]{0,20}來"),
    re.compile(r"對於[^。！？\n]{0,20}進行"),
    re.compile(r"被[^。！？\n]{0,12}所[^以有]"),
    re.compile(r"超過[^。！？\n]{0,12}以上"),
    re.compile(r"至少[^。！？\n]{0,12}以上"),
)

RUST_STRING = re.compile(r'r#*".*?"#*|"(?:[^"\\]|\\.)*"', re.S)


@dataclass
class Report:
    files: int = 0
    fixes: list[dict] = field(default_factory=list)
    review: list[dict] = field(default_factory=list)


def files() -> list[Path]:
    found: list[Path] = []
    for name in SCAN_FILES:
        path = ROOT / name
        if path.is_file():
            found.append(path)
    for name in SCAN_DIRS:
        base = ROOT / name
        if not base.exists():
            continue
        for path in base.rglob("*"):
            if not path.is_file() or path.suffix not in EXTS:
                continue
            if any(part in SKIP_DIRS for part in path.relative_to(ROOT).parts):
                continue
            if path.relative_to(ROOT).as_posix() in SKIP_FILES:
                continue
            found.append(path)
    return sorted(set(found))


def replace_terms(text: str) -> tuple[str, list[tuple[str, str, int]]]:
    changes: list[tuple[str, str, int]] = []
    for bad, good in TERMS:
        count = text.count(bad)
        if count:
            text = text.replace(bad, good)
            changes.append((bad, good, count))
    return text, changes


def clean_span(text: str) -> tuple[str, list[str], list[str]]:
    """回傳（新字、修正說明、待人工看的片段）。"""
    notes: list[str] = []
    review: list[str] = []
    text, terms = replace_terms(text)
    notes += [f"{bad}→{good}（{count}）" for bad, good, count in terms]
    for pattern, good in GRAMMAR:
        text, count = pattern.subn(good, text)
        if count:
            notes.append(f"重複／累贅：{pattern.pattern}→{good}（{count}）")
    text, count = PUNCT.subn(lambda m: next(g for g in m.groups() if g), text)
    if count:
        notes.append(f"重複標點（{count}）")
    for pattern in REVIEW:
        for match in pattern.finditer(text):
            review.append(match.group(0))
    return text, notes, review


def lint_rust(text: str) -> tuple[str, list[str], list[str]]:
    notes: list[str] = []
    review: list[str] = []
    pieces: list[str] = []
    last = 0
    for match in RUST_STRING.finditer(text):
        pieces.append(text[last : match.start()])
        span, span_notes, span_review = clean_span(match.group(0))
        pieces.append(span)
        notes += span_notes
        review += span_review
        last = match.end()
    pieces.append(text[last:])
    return "".join(pieces), notes, review


def run(fix: bool) -> Report:
    report = Report()
    for path in files():
        raw = path.read_text(encoding="utf-8")
        if path.suffix == ".rs":
            new, notes, review = lint_rust(raw)
        else:
            new, notes, review = clean_span(raw)
        report.files += 1
        if review:
            report.review.append({"file": str(path.relative_to(ROOT)), "text": review})
        if new != raw:
            report.fixes.append(
                {
                    "file": str(path.relative_to(ROOT)),
                    "notes": notes,
                }
            )
            if fix:
                path.write_text(new, encoding="utf-8", newline="")
    return report


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fix", action="store_true", help="直接寫回修正")
    parser.add_argument("--check", action="store_true", help="只檢查，有問題回傳 1")
    parser.add_argument("--report", help="輸出 JSON 報告")
    args = parser.parse_args()
    report = run(args.fix)
    summary = {
        "files": report.files,
        "fixed_files": len(report.fixes),
        "fixes": report.fixes,
        "review": report.review,
    }
    if args.report:
        target = Path(args.report)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"中文自檢：掃描 {report.files} 檔，修正 {len(report.fixes)} 檔，待人工確認 {len(report.review)} 檔")
    for item in report.fixes:
        print(f"- {item['file']}: {'；'.join(item['notes'])}")
    for item in report.review:
        print(f"  待看 {item['file']}: {item['text'][0]}")
    if args.check and (report.fixes or report.review):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
