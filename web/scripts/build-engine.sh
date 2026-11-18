#!/bin/sh
# Builds the Slide engine (bindings/wasm) to WebAssembly into
# public/engine, where the editor loads it from its own package. Needs wasm-pack.
set -eu
web="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$web/.." && pwd)"
if toolchain="$(rustc --print sysroot 2>/dev/null)" && [ -d "$toolchain/lib" ]; then
  export DYLD_FALLBACK_LIBRARY_PATH="$toolchain/lib${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}"
fi
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo/target}"
# Some dependencies compile C for the target (psm, through loro); Apple's clang cannot target
# WebAssembly, so use Homebrew's LLVM when it is there and no compiler is set.
if [ -z "${CC_wasm32_unknown_unknown:-}" ] && command -v brew >/dev/null 2>&1; then
  for formula in llvm@21 llvm; do
    llvm="$(brew --prefix "$formula" 2>/dev/null || true)"
    if [ -n "$llvm" ] && [ -x "$llvm/bin/clang" ]; then
      export CC_wasm32_unknown_unknown="$llvm/bin/clang" AR_wasm32_unknown_unknown="$llvm/bin/llvm-ar"
      break
    fi
  done
fi
cd "$repo"
wasm-pack build bindings/wasm --target web --release --no-typescript --out-dir "$web/public/engine"
rm -f "$web/public/engine/.gitignore" "$web/public/engine/package.json"
# Simplified Chinese needs more than the bundled Japanese face: the editor gives the engine this
# one (from the kition-fonts checkout next to the repository, or KITION_FONTS_DIR) before it opens.
# Not Noto CJK, which AGENTS.md rules out.
fonts="${KITION_FONTS_DIR:-$repo/../kition-fonts}/fonts/droid-sans-fallback"
rm -rf "$web/public/fonts"
if [ -f "$fonts/DroidSansFallbackFull.ttf" ]; then
  mkdir -p "$web/public/fonts"
  cp "$fonts/DroidSansFallbackFull.ttf" "$web/public/fonts/"
  cp "$fonts/NOTICE" "$web/public/fonts/DroidSansFallback-NOTICE.txt"
else
  echo "warning: no kition-fonts checkout at $fonts; some Chinese text will show as boxes" >&2
fi
# Open, metric-compatible stand-ins for Calibri, Cambria, Arial, Times New Roman and Courier New
# (open decision F1): text set in those fonts breaks into the lines Office breaks it into.
metric="${KITION_FONTS_DIR:-$repo/../kition-fonts}/fonts"
for f in carlito/Carlito-Regular carlito/Carlito-Bold carlito/Carlito-Italic carlito/Carlito-BoldItalic \
  caladea/Caladea-Regular caladea/Caladea-Bold arimo/Arimo-Variable arimo/Arimo-Italic-Variable \
  tinos/Tinos-Regular tinos/Tinos-Bold cousine/Cousine-Regular cousine/Cousine-Bold; do
  if [ -f "$metric/$f.ttf" ]; then
    mkdir -p "$web/public/fonts"
    cp "$metric/$f.ttf" "$web/public/fonts/"
    cp "$metric/${f%%/*}/OFL.txt" "$web/public/fonts/${f%%/*}-OFL.txt"
  else
    echo "warning: no $f in the kition-fonts checkout; its documents' text will reflow" >&2
  fi
done
