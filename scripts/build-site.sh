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
# The tutorials moved: old URLs go to their new place.
mkdir -p "$out/tutorials"
stub() { # old new
  mkdir -p "$out/$(dirname "$1")"
  printf '<!doctype html><meta charset="utf-8"><meta http-equiv="refresh" content="0; url=/Hardroxide/book/%s"><link rel="canonical" href="/Hardroxide/book/%s"><a href="/Hardroxide/book/%s">Moved</a>\n' "$2" "$2" "$2" > "$out/$1"
}
stub tutorials/index.html tutorials/index.html
# Old page -> new page, both without `.html`, relative to tutorials/. In 3.0 the first tutorials moved to the base
# overlay and those on their own designs to own-design/.
moved="linux/01-gpio:linux/01-lights
linux/02-timer-spi-irq:own-design/01-timer-spi-irq
linux/03-uart-iic-loopback:own-design/02-uart-iic-loopback
linux/04-hls-vadd:own-design/03-hls-vadd
linux/05-hls-pointers:own-design/04-hls-pointers
linux/06-dma:own-design/05-dma
linux/07-interrupt-controller:linux/02-interrupts
linux/08-bram-uart:linux/03-block-ram
linux/09-stream-fifo:own-design/06-stream-fifo
linux/10-update:own-design/07-update
linux/11-base-overlay:linux/04-base-overlay
baremetal/02-timer-interrupt:own-design/08-baremetal-timer
baremetal/03-dma:own-design/09-baremetal-dma"
for pair in $moved; do
  old=${pair%%:*}; new=${pair#*:}
  [ -e "$out/book/tutorials/$new.html" ] || { echo "no page to redirect to: $new" >&2; exit 1; }
  # Inside the book, and at the flat address from before the per-flavor folders (Linux pages only).
  stub "book/tutorials/$old.html" "tutorials/$new.html"
  case $old in linux/*) stub "tutorials/${old#linux/}.html" "tutorials/$new.html" ;; esac
done
stub tutorials/12-baremetal.html tutorials/baremetal/01-jtag.html
stub tutorials/13-rtos-control.html tutorials/rtos/01-control-loop.html
touch "$out/.nojekyll"
echo "site in $out"
