#!/usr/bin/env bash
# Run the bare-metal smoke test (baremetal/ps7-smoke) on QEMU's Zynq-7000 machine and check
# what it prints: startup, console, clocks, time source, GIC and private-timer interrupts,
# and the exception report. QEMU does not model the PL; this tests the PS support only.
#
# QEMU: $QEMU_SYSTEM, or `qemu-system-aarch64` on PATH, or the one bundled with Vitis
# (which has the `xilinx-zynq-a9` machine; stock Debian/Ubuntu QEMU has `xilinx-zynq-a9` too).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/baremetal/ps7-smoke"
XILINX_ROOT=${XILINX_ROOT:-$HOME/AMD/2025.2}
VITIS_QEMU_DIR=$XILINX_ROOT/data/emulation/qemu/comp/qemu/sysroots/x86_64-petalinux-linux/usr
if [ -n "${QEMU_SYSTEM:-}" ]; then
  RUN=("$QEMU_SYSTEM")
elif command -v qemu-system-arm >/dev/null; then
  RUN=(qemu-system-arm)
elif [ -x "$VITIS_QEMU_DIR/bin/qemu-system-aarch64" ]; then
  # The bundled QEMU needs its own libraries, for this process only.
  RUN=(env "LD_LIBRARY_PATH=$VITIS_QEMU_DIR/lib:$VITIS_QEMU_DIR/../lib" "$VITIS_QEMU_DIR/bin/qemu-system-aarch64")
else
  echo "no QEMU with the xilinx-zynq-a9 machine (set QEMU_SYSTEM)" >&2; exit 1
fi
rustup target add armv7a-none-eabihf >/dev/null
cargo build --release --features qemu
ELF=target/armv7a-none-eabihf/release/ps7-smoke
OUT=$(mktemp)
trap 'rm -f "$OUT"' EXIT
timeout "${QEMU_TIMEOUT:-20}" "${RUN[@]}" -M xilinx-zynq-a9 -m 512 -display none \
  -serial stdio -serial null -monitor none -kernel "$ELF" >"$OUT" 2>&1 || true
cat "$OUT"
fail=0
need() { grep -q -- "$1" "$OUT" || { echo "MISSING: $1" >&2; fail=1; }; }
need "== hardroxide-ps7 smoke test =="
need "core 0"
need "timer interrupts: 20 in"
need "unhandled interrupts: 0"
need "ALL OK"
need "EXCEPTION: undefined instruction"
# Timing: sleep(100 ms) within 5%, 20 ticks of 10 ms within 5% of 200 ms (virtual time).
python3 - "$OUT" <<'PY' || fail=1
import re, sys
t = open(sys.argv[1], errors="replace").read()
sleep_us = int(re.search(r"sleep\(100 ms\) took (\d+) us", t).group(1))
span_us = int(re.search(r"timer interrupts: 20 in (\d+) us", t).group(1))
ok = 95_000 <= sleep_us <= 105_000 and 190_000 <= span_us <= 210_000
print(f"timing: sleep {sleep_us} us, 20 ticks {span_us} us -> {'ok' if ok else 'OUT OF RANGE'}")
sys.exit(0 if ok else 1)
PY
[ "$fail" = 0 ] && echo "qemu smoke test: ok" || { echo "qemu smoke test: FAILED" >&2; exit 1; }

# ---- the kernel (hardroxide-rt) ----
cd "$ROOT/baremetal/rt-smoke"
cargo build --release --features qemu
OUT2=$(mktemp)
trap 'rm -f "$OUT" "$OUT2"' EXIT
timeout "${QEMU_TIMEOUT:-20}" "${RUN[@]}" -M xilinx-zynq-a9 -m 512 -display none \
  -serial stdio -serial null -monitor none -kernel target/armv7a-none-eabihf/release/rt-smoke >"$OUT2" 2>&1 || true
cat "$OUT2"
fail=0
need2() { grep -q -- "$1" "$OUT2" || { echo "MISSING: $1" >&2; fail=1; }; }
need2 "no drift"
need2 "10/10 items received in order"
need2 "woke the task: true, after 20 ticks"
need2 "0 late"
need2 "inheritance worked"
need2 "second (after rearm) true, nothing pending true"
need2 "a take that would wait false"
need2 "main went on and no task ran"
need2 "0 broken (preserved)"
need2 "RT ALL OK"
need2 "EXCEPTION: undefined instruction"
[ "$fail" = 0 ] && echo "qemu kernel test: ok" || { echo "qemu kernel test: FAILED" >&2; exit 1; }

# ---- the SD host driver and the FAT32 file system (baremetal/sd-smoke) ----
# QEMU's Zynq machine has an SD host controller; the "card" is a throwaway image the program formats,
# fills and reads back. Afterwards the host's fsck.fat checks the file system the program wrote.
cd "$ROOT/baremetal/sd-smoke"
cargo build --release --features qemu
IMG=$(mktemp)
OUT3=$(mktemp)
trap 'rm -f "$OUT" "$OUT2" "$OUT3" "$IMG" "$IMG.part"' EXIT
truncate -s 128M "$IMG"
timeout "${QEMU_TIMEOUT:-20}" "${RUN[@]}" -M xilinx-zynq-a9 -m 512 -display none \
  -serial stdio -serial null -monitor none -drive "if=sd,index=0,file=$IMG,format=raw" \
  -kernel target/armv7a-none-eabihf/release/sd-smoke >"$OUT3" 2>&1 || true
cat "$OUT3"
fail=0
need3() { grep -q -- "$1" "$OUT3" || { echo "MISSING: $1" >&2; fail=1; }; }
need3 "raw blocks: 4 blocks written and read back"
need3 "formatted and mounted"
need3 "SD SMOKE OK"
if command -v fsck.fat >/dev/null; then
  # The partition starts at sector 2048 (1 MiB).
  dd if="$IMG" of="$IMG.part" bs=512 skip=2048 status=none
  if fsck.fat -n "$IMG.part" >/dev/null; then echo "fsck.fat on the card image: clean"; else echo "fsck.fat found problems" >&2; fail=1; fi
else
  echo "(fsck.fat not installed; skipping the file system check)"
fi
[ "$fail" = 0 ] && echo "qemu SD test: ok" || { echo "qemu SD test: FAILED" >&2; exit 1; }

# ---- the Ethernet MAC driver and the network stack (baremetal/net-smoke) ----
# QEMU's Zynq machine has the Cadence GEM and user-mode networking (a virtual router that answers DHCP and forwards host ports to the
# guest). The program gets an address over DHCP and serves HTTP (80), a TCP echo (9) and a UDP echo (7); the host talks to it with curl
# and Python sockets, so the stack is checked against independent implementations of the protocols.
cd "$ROOT/baremetal/net-smoke"
cargo build --release --features qemu
OUT4=$(mktemp)
trap 'rm -f "$OUT" "$OUT2" "$OUT3" "$OUT4" "$IMG" "$IMG.part"' EXIT
"${RUN[@]}" -M xilinx-zynq-a9 -m 512 -display none -serial stdio -serial null -monitor none \
  -nic "user,model=cadence_gem,mac=02:48:52:58:00:01,hostfwd=tcp:127.0.0.1:18080-:80,hostfwd=tcp:127.0.0.1:18009-:9,hostfwd=tcp:127.0.0.1:18443-:443,hostfwd=udp:127.0.0.1:18007-:7" \
  -kernel target/armv7a-none-eabihf/release/net-smoke >"$OUT4" 2>&1 &
QPID=$!
fail=0
for _ in $(seq 1 100); do grep -q "NET READY" "$OUT4" && break; sleep 0.1; done
cat "$OUT4"
grep -q "NET READY ip=10.0.2.15 dhcp=true" "$OUT4" || { echo "MISSING: an address from DHCP" >&2; fail=1; }
if [ "$fail" = 0 ]; then
python3 - <<'PY' || fail=1
import hashlib, socket, subprocess, sys, time, urllib.request

def expected(n, arg=5):
    return bytes((((i * 2654435761 + arg) & 0xFFFFFFFF) << 7 | ((i * 2654435761 + arg) & 0xFFFFFFFF) >> 25) & 0xFF for i in range(n))

ok = True
def check(name, cond, detail=""):
    global ok
    print(("PASS  " if cond else "FAIL  ") + name + (" " + detail if detail and not cond else ""))
    ok = ok and cond

# A page, with curl.
page = subprocess.run(["curl", "-s", "-m", "10", "-i", "http://127.0.0.1:18080/"], capture_output=True, text=True).stdout
check("GET / with curl", page.startswith("HTTP/1.1 200 OK") and "hello from the board" in page, page[:80])
# Not found.
code = subprocess.run(["curl", "-s", "-m", "10", "-o", "/dev/null", "-w", "%{http_code}", "http://127.0.0.1:18080/nothing"], capture_output=True, text=True).stdout
check("404 for an unknown path", code == "404", code)
# A 300 kB download, byte for byte.
t0 = time.time()
body = subprocess.run(["curl", "-s", "-m", "60", "http://127.0.0.1:18080/big"], capture_output=True).stdout
dt = time.time() - t0
check("300000 byte download is exact", body == expected(300000), "got %d bytes, sha %s" % (len(body), hashlib.sha256(body).hexdigest()[:12]))
print("      %.1f s for 300 kB (QEMU, emulated CPU)" % dt)
# A POST echoed back.
payload = bytes(range(256)) * 2
echo = subprocess.run(["curl", "-s", "-m", "10", "--data-binary", "@-", "-H", "Content-Type: application/octet-stream", "http://127.0.0.1:18080/echo"], input=payload, capture_output=True).stdout
check("POST body echoed", echo == payload, "got %d bytes" % len(echo))
# Many requests in a row (sockets are reused).
bad = 0
for i in range(40):
    r = subprocess.run(["curl", "-s", "-m", "10", "-o", "/dev/null", "-w", "%{http_code}", "http://127.0.0.1:18080/"], capture_output=True, text=True).stdout
    bad += r != "200"
check("40 requests in a row", bad == 0, "%d failed" % bad)
# Several at once.
ps = [subprocess.Popen(["curl", "-s", "-m", "30", "-o", "/dev/null", "-w", "%{http_code}", "http://127.0.0.1:18080/"], stdout=subprocess.PIPE, text=True) for _ in range(6)]
codes = [p.communicate()[0] for p in ps]
check("6 concurrent requests", all(c == "200" for c in codes), str(codes))
# UDP echo.
u = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
u.settimeout(5)
good = 0
for n in (1, 100, 512):
    msg = bytes([n % 251]) * n
    u.sendto(msg, ("127.0.0.1", 18007))
    try:
        good += u.recvfrom(2048)[0] == msg
    except socket.timeout:
        pass
check("UDP echo of 1, 100 and 512 bytes", good == 3, "%d of 3" % good)
# TCP echo (a raw stream, 100000 bytes both ways).
s = socket.create_connection(("127.0.0.1", 18009), timeout=20)
data = bytes((i * 7) & 0xFF for i in range(100000))
s.setblocking(True)
got = b""
sent = 0
s.settimeout(0.05)
t0 = time.time()
while len(got) < len(data) and time.time() - t0 < 60:
    if sent < len(data):
        try:
            sent += s.send(data[sent:sent + 4096])
        except socket.timeout:
            pass
    try:
        chunk = s.recv(65536)
        if not chunk:
            break
        got += chunk
    except socket.timeout:
        pass
s.close()
check("TCP echo of 100000 bytes", got == data, "got %d bytes" % len(got))
# ---- HTTPS (TLS 1.3, ChaCha20-Poly1305, X25519, an Ed25519 certificate) ----
S = "https://127.0.0.1:18443"
def curl(*args, **kw):
    return subprocess.run(["curl", "-sk", "-m", "30", "--tlsv1.3", *args], capture_output=True, **kw)
r = curl("-w", "\n%{http_code}", S + "/")
page = r.stdout.decode()
check("HTTPS GET / with curl", page.startswith("hello over TLS 1.3 from the board") and page.rstrip().endswith("200"), repr(page[:80]) + " curl: " + r.stderr.decode()[:200] + " rc=%d" % r.returncode)
code = curl("-o", "/dev/null", "-w", "%{http_code}", S + "/nothing").stdout.decode()
check("HTTPS 404", code == "404", code)
body = curl(S + "/big").stdout
check("HTTPS 100000 byte download is exact", body == expected(100000), "got %d bytes" % len(body))
echo = curl("--data-binary", "@-", "-H", "Content-Type: application/octet-stream", S + "/echo", input=payload).stdout
check("HTTPS POST body echoed", echo == payload, "got %d bytes" % len(echo))
bad = sum(curl("-o", "/dev/null", "-w", "%{http_code}", S + "/").stdout.decode() != "200" for _ in range(30))
check("30 HTTPS requests in a row (a new handshake each)", bad == 0, "%d failed" % bad)
ps = [subprocess.Popen(["curl", "-sk", "-m", "60", "--tlsv1.3", "-o", "/dev/null", "-w", "%{http_code}", S + "/"], stdout=subprocess.PIPE, text=True) for _ in range(6)]
codes = [p.communicate()[0] for p in ps]
check("6 concurrent HTTPS requests (4 sockets)", all(c == "200" for c in codes), str(codes))
# OpenSSL's own client: the negotiated parameters.
sc = subprocess.run(["openssl", "s_client", "-connect", "127.0.0.1:18443", "-tls1_3", "-ciphersuites", "TLS_CHACHA20_POLY1305_SHA256", "-brief", "-ign_eof"],
                    input=b"GET / HTTP/1.0\r\n\r\n", capture_output=True, timeout=60)
txt = (sc.stdout + sc.stderr).decode()
# OpenSSL names the signature scheme in full or, in some versions, as type and hash; in TLS 1.3 ECDSA with SHA-256 is P-256.
p256 = "ecdsa_secp256r1_sha256" in txt or ("Signature type: ECDSA" in txt and "Hash used: SHA256" in txt)
check("openssl s_client: TLS 1.3, ChaCha20-Poly1305, an ECDSA P-256 signature, X25519", all(w in txt for w in ("TLSv1.3", "TLS_CHACHA20_POLY1305_SHA256", "X25519")) and p256 and "hello over TLS" in txt, txt[-300:])
# A client that offers only AES-GCM is refused with an alert; the server is unharmed.
sc = subprocess.run(["openssl", "s_client", "-connect", "127.0.0.1:18443", "-tls1_3", "-ciphersuites", "TLS_AES_128_GCM_SHA256"], input=b"", capture_output=True, timeout=60)
check("a client without ChaCha20-Poly1305 is refused", b"alert" in sc.stderr + sc.stdout or b"handshake failure" in sc.stderr + sc.stdout, (sc.stderr + sc.stdout).decode()[-200:])
# Garbage to the TLS port, and a connection that sends nothing, must not stop the server.
g = socket.create_connection(("127.0.0.1", 18443), timeout=10)
g.sendall(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n" + bytes(range(256)) * 4)
time.sleep(1)
g.close()
idle = socket.create_connection(("127.0.0.1", 18443), timeout=10)
r = curl("-w", "\n%{http_code}", S + "/")
page = r.stdout.decode()
idle.close()
check("garbage and an idle connection do not stop the TLS server", page.rstrip().endswith("200"), repr(page[:80]) + " curl: " + r.stderr.decode()[:200] + " rc=%d" % r.returncode)
# Plain HTTP still works beside it.
page = subprocess.run(["curl", "-s", "-m", "10", "http://127.0.0.1:18080/"], capture_output=True, text=True).stdout
check("plain HTTP still served", "hello from the board" in page, page[:80])
sys.exit(0 if ok else 1)
PY
fi
kill "$QPID" 2>/dev/null; wait "$QPID" 2>/dev/null || true
[ "$fail" = 0 ] && echo "qemu network test: ok" || { echo "qemu network test: FAILED" >&2; exit 1; }

# ---- Zynq UltraScale+ (hardroxide-psu), on QEMU's AArch64 machine ----
"$ROOT/scripts/test-qemu-psu.sh"
