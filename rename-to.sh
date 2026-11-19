#!/usr/bin/env bash
# 用法：./rename-to.sh <新中文名> <新英文名> <新repo名>
# 例：  ./rename-to.sh 島光 IsletGlow islet-glow
#
# 做的事：把上游「島仔 Daozai」的品牌字串換成你的新品牌。
# 保留：LICENSE、上游作者版權聲明（MIT 要求，勿刪）。
set -euo pipefail

ZH="${1:?需要新中文名}"; EN="${2:?需要新英文名}"; REPO="${3:?需要新 repo 名}"
CRATE="$(echo "$EN" | tr '[:upper:]' '[:lower:]' | tr -cd 'a-z0-9-')"

echo "→ 中文名: $ZH / 英文名: $EN / crate: $CRATE / repo: $REPO"

# 1) 品牌字串（顯示用）
grep -rl --include='*.rs' --include='*.rc' --include='*.nsi' --include='*.ps1' --include='*.cmd' \
     --include='*.md' --include='*.toml' --include='*.yml' -e '島仔' -e '岛仔' . 2>/dev/null \
  | while read -r f; do
      sed -i "s/島仔/$ZH/g; s/岛仔/$ZH/g" "$f"
    done

grep -rl --include='*.rs' --include='*.rc' --include='*.nsi' --include='*.ps1' --include='*.cmd' \
     --include='*.md' --include='*.toml' --include='*.yml' -e 'Daozai' -e 'DAOZAI' . 2>/dev/null \
  | while read -r f; do
      sed -i "s/DAozai/PLACEHOLDER/g; s/Daozai/$EN/g; s/DAOZAI/$(echo "$EN" | tr '[:lower:]' '[:upper:]')/g; s/PLACEHOLDER/Daozai/g" "$f"
    done

# 2) Cargo 套件名（crate 名只能是 a-z0-9-_）
sed -i "0,/^name = \"daozai\"/s//name = \"$CRATE\"/" Cargo.toml
sed -i "0,/^name = \"daozai\"/s//name = \"$CRATE\"/" Cargo.toml   # [[bin]] 同名
sed -i "s/^\[\[bin\]\]/[[bin]]/" Cargo.toml

# 3) 執行檔名（exe 名稱、單實例判斷都會跟著變）
grep -rl --include='*.rs' -e '"daozai.exe"' . 2>/dev/null \
  | while read -r f; do sed -i "s/\"daozai.exe\"/\"$CRATE.exe\"/g" "$f"; done

# 4) 版本資源與安裝器
sed -i "s/OriginalFilename.*Daozai.*exe/OriginalFilename \"$EN.exe\"/" app.rc 2>/dev/null || true
sed -i "s/daozai.ico/$CRATE.ico/g" app.rc installer/setup.nsi 2>/dev/null || true
[ -f "assets/daozai.ico" ] && cp "assets/daozai.ico" "assets/$CRATE.ico"
grep -rl --include='*.nsi' -e 'daozai' installer 2>/dev/null \
  | while read -r f; do sed -i "s/daozai/$CRATE/g" "$f"; done

# 5) README / 授權聲明：保留上游出處
if [ -f NOTICE ]; then :; else
cat > NOTICE <<EOF
本專案衍生自 Renly1994/xiaotang-daozai（MIT License），
原作者：小唐（Renly1994）。台灣繁體化與品牌化由 $(whoami) 維護。
原始授權條款見 LICENSE，依 MIT 要求保留原作者版權聲明。
EOF
fi

echo "✔ 完成。接下來："
echo "   cargo check --lib"
echo "   git add -A && git commit -m \"brand: $ZH ($EN)\""
echo "   git remote set-url origin git@github.com:<你的帳號>/$REPO.git && git push -u origin main"
echo "   git tag v0.6.66-tw1 && git push --tags   # 觸發 .github/workflows/release.yml"
