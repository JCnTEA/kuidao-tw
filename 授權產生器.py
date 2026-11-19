#!/usr/bin/env python3
"""鬼島 授權產生器（僅供本人使用，請勿外流）

用法：
    python 授權產生器.py                 # 互動輸入機器碼
    python 授權產生器.py A1B2-C3D4-E5F6-0708

程式首次啟動若未授權，會顯示「本機機器碼」，複製貼進來即可得到授權碼。
再執行：kuidao.exe --license <授權碼>
"""
import sys

# 必須與 src/license.rs 的 SECRET 完全一致，否則算出來的碼驗不過。
SECRET = "KUIDAO-TW-2026-OWNER-ONLY-5b8e41c7"

MASK = 0xFFFFFFFFFFFFFFFF
FNV_OFFSET = 0xCBF29CE484222325
FNV_PRIME = 0x100000001B3


def fnv64(data: bytes) -> int:
    h = FNV_OFFSET
    for b in data:
        h ^= b
        h = (h * FNV_PRIME) & MASK
    return h


def machine_id(raw: str) -> str:
    return "".join(ch for ch in raw if ch.isalnum()).upper()


def code_for(mid: str) -> str:
    h = fnv64(f"{SECRET}:{mid}".encode())
    hexs = f"{h:016x}".upper()
    return "-".join(hexs[i:i + 4] for i in range(0, 16, 4))


def main() -> None:
    if len(sys.argv) > 1:
        raw = " ".join(sys.argv[1:])
    else:
        raw = input("請輸入機器碼（例：A1B2-C3D4-E5F6-0708）：")
    mid = machine_id(raw)
    if len(mid) != 16:
        print(f"警告：機器碼長度為 {len(mid)}，正常應為 16 碼，請確認是否複製完整。")
    print("\n機器碼 :", "-".join(mid[i:i + 4] for i in range(0, len(mid), 4)))
    print("授權碼 :", code_for(mid))
    print("\n啟用指令：kuidao.exe --license", code_for(mid))


if __name__ == "__main__":
    main()
