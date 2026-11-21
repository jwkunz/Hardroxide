#!/usr/bin/env bash
# Assemble the GitHub Pages site into target/site:
#   /          landing page (site/)
#   /book/     the mdBook
#   /api/      rustdoc for every crate
# Old book URLs (/guide/x.html, ...) get redirect stubs so existing links keep working.
set -euo pipefail
cd "$(dirname "$0")/.."

out=target/site
rm -rf "$out"
mkdir -p "$out"
cp -r site/. "$out/"

mdbook build
cp -r target/book "$out/book"

cargo doc --workspace --no-deps
cp -r target/doc "$out/api"
cat > "$out/api/index.html" <<'HTML'
<!doctype html><meta charset="utf-8"><title>Hardroxide API reference</title>
<meta http-equiv="refresh" content="0; url=hardroxide_gen/index.html">
<p>Redirecting to <a href="hardroxide_gen/index.html">hardroxide_gen</a>; the other crates are in the sidebar and on the <a href="../index.html">home page</a>.</p>
HTML

# Redirect stubs for pre-landing-page URLs.
(cd "$out/book" && find . -name '*.html' ! -name index.html ! -name 404.html ! -path './tutorials/*' ! -path './print.html' | while read -r f; do
  f=${f#./}
  mkdir -p "../$(dirname "$f")"
  [ -e "../$f" ] || printf '<!doctype html><meta charset="utf-8"><meta http-equiv="refresh" content="0; url=/Hardroxide/book/%s"><link rel="canonical" href="/Hardroxide/book/%s"><a href="/Hardroxide/book/%s">Moved</a>\n' "$f" "$f" "$f" > "../$f"
done)
# The tutorials moved into per-flavor folders: old flat URLs go to their new place.
mkdir -p "$out/tutorials"
stub() { # old new
  mkdir -p "$out/$(dirname "$1")"
  printf '<!doctype html><meta charset="utf-8"><meta http-equiv="refresh" content="0; url=/Hardroxide/book/%s"><link rel="canonical" href="/Hardroxide/book/%s"><a href="/Hardroxide/book/%s">Moved</a>\n' "$2" "$2" "$2" > "$out/$1"
}
stub tutorials/index.html tutorials/index.html
for f in $(cd "$out/book/tutorials/linux" && ls [0-9]*.html); do stub "tutorials/$f" "tutorials/linux/$f"; done
stub tutorials/12-baremetal.html tutorials/baremetal/01-jtag.html
stub tutorials/13-rtos-control.html tutorials/rtos/01-control-loop.html
touch "$out/.nojekyll"
echo "site in $out"
