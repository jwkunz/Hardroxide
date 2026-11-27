#!/usr/bin/env python3
"""Checks a board (or QEMU) running baremetal/net-smoke with independent implementations of the protocols: curl, Python
sockets and openssl. Used by scripts/test-qemu.sh and scripts/test-board-rfsoc.sh.

Usage: net_check.py <host> <http port> <tcp echo port> <https port> <udp echo port> [--figures]
--figures also measures throughput and requests per second (meaningful on a board, not in an emulator).
"""
import hashlib, socket, subprocess, sys, time

HOST = sys.argv[1]
HTTP, TCP_ECHO, HTTPS, UDP_ECHO = (int(v) for v in sys.argv[2:6])
H = "http://%s:%d" % (HOST, HTTP)

def expected(n, arg=5):
    return bytes((((i * 2654435761 + arg) & 0xFFFFFFFF) << 7 | ((i * 2654435761 + arg) & 0xFFFFFFFF) >> 25) & 0xFF for i in range(n))

ok = True
def check(name, cond, detail=""):
    global ok
    print(("PASS  " if cond else "FAIL  ") + name + (" " + detail if detail and not cond else ""))
    ok = ok and cond

# A page, with curl.
page = subprocess.run(["curl", "-s", "-m", "10", "-i", H + "/"], capture_output=True, text=True).stdout
check("GET / with curl", page.startswith("HTTP/1.1 200 OK") and "hello from the board" in page, page[:80])
# Not found.
code = subprocess.run(["curl", "-s", "-m", "10", "-o", "/dev/null", "-w", "%{http_code}", H + "/nothing"], capture_output=True, text=True).stdout
check("404 for an unknown path", code == "404", code)
# A 300 kB download, byte for byte.
t0 = time.time()
body = subprocess.run(["curl", "-s", "-m", "60", H + "/big"], capture_output=True).stdout
dt = time.time() - t0
check("300000 byte download is exact", body == expected(300000), "got %d bytes, sha %s" % (len(body), hashlib.sha256(body).hexdigest()[:12]))
print("      %.2f s for 300 kB" % dt)
# A POST echoed back.
payload = bytes(range(256)) * 2
echo = subprocess.run(["curl", "-s", "-m", "10", "--data-binary", "@-", "-H", "Content-Type: application/octet-stream", H + "/echo"], input=payload, capture_output=True).stdout
check("POST body echoed", echo == payload, "got %d bytes" % len(echo))
# Many requests in a row (sockets are reused).
bad = 0
for i in range(40):
    r = subprocess.run(["curl", "-s", "-m", "10", "-o", "/dev/null", "-w", "%{http_code}", H + "/"], capture_output=True, text=True).stdout
    bad += r != "200"
check("40 requests in a row", bad == 0, "%d failed" % bad)
# Several at once.
ps = [subprocess.Popen(["curl", "-s", "-m", "30", "-o", "/dev/null", "-w", "%{http_code}", H + "/"], stdout=subprocess.PIPE, text=True) for _ in range(6)]
codes = [p.communicate()[0] for p in ps]
check("6 concurrent requests", all(c == "200" for c in codes), str(codes))
# UDP echo.
u = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
u.settimeout(5)
good = 0
for n in (1, 100, 512):
    msg = bytes([n % 251]) * n
    u.sendto(msg, (HOST, UDP_ECHO))
    try:
        good += u.recvfrom(2048)[0] == msg
    except socket.timeout:
        pass
check("UDP echo of 1, 100 and 512 bytes", good == 3, "%d of 3" % good)
# TCP echo (a raw stream, 100000 bytes both ways).
s = socket.create_connection((HOST, TCP_ECHO), timeout=20)
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
S = "https://%s:%d" % (HOST, HTTPS)
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
sc = subprocess.run(["openssl", "s_client", "-connect", "%s:%d" % (HOST, HTTPS), "-tls1_3", "-ciphersuites", "TLS_CHACHA20_POLY1305_SHA256", "-brief", "-ign_eof"],
                    input=b"GET / HTTP/1.0\r\n\r\n", capture_output=True, timeout=60)
txt = (sc.stdout + sc.stderr).decode()
# OpenSSL names the signature scheme in full or, in some versions, as type and hash; in TLS 1.3 ECDSA with SHA-256 is P-256.
p256 = "ecdsa_secp256r1_sha256" in txt or ("Signature type: ECDSA" in txt and "Hash used: SHA256" in txt)
check("openssl s_client: TLS 1.3, ChaCha20-Poly1305, an ECDSA P-256 signature, X25519", all(w in txt for w in ("TLSv1.3", "TLS_CHACHA20_POLY1305_SHA256", "X25519")) and p256 and "hello over TLS" in txt, txt[-300:])
# A client that offers only AES-GCM is refused with an alert; the server is unharmed.
sc = subprocess.run(["openssl", "s_client", "-connect", "%s:%d" % (HOST, HTTPS), "-tls1_3", "-ciphersuites", "TLS_AES_128_GCM_SHA256"], input=b"", capture_output=True, timeout=60)
check("a client without ChaCha20-Poly1305 is refused", b"alert" in sc.stderr + sc.stdout or b"handshake failure" in sc.stderr + sc.stdout, (sc.stderr + sc.stdout).decode()[-200:])
# Garbage to the TLS port, and a connection that sends nothing, must not stop the server.
g = socket.create_connection((HOST, HTTPS), timeout=10)
g.sendall(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n" + bytes(range(256)) * 4)
time.sleep(1)
g.close()
idle = socket.create_connection((HOST, HTTPS), timeout=10)
r = curl("-w", "\n%{http_code}", S + "/")
page = r.stdout.decode()
idle.close()
check("garbage and an idle connection do not stop the TLS server", page.rstrip().endswith("200"), repr(page[:80]) + " curl: " + r.stderr.decode()[:200] + " rc=%d" % r.returncode)
# Plain HTTP still works beside it.
page = subprocess.run(["curl", "-s", "-m", "10", H + "/"], capture_output=True, text=True).stdout
check("plain HTTP still served", "hello from the board" in page, page[:80])
if "--figures" in sys.argv:
    def timed(n, url, *args):
        t0 = time.time()
        good = sum(subprocess.run(["curl", "-sk", "-m", "30", "-o", "/dev/null", "-w", "%{http_code}", *args, url], capture_output=True, text=True).stdout == "200" for _ in range(n))
        return good, time.time() - t0
    # One curl process per request, so the rates include its start-up (a few milliseconds).
    size = len(subprocess.run(["curl", "-s", "-m", "60", H + "/big"], capture_output=True).stdout)
    good, dt = timed(100, H + "/big")
    print("FIGURE HTTP download: %d of 100 x %d bytes in %.2f s = %.2f MB/s" % (good, size, dt, good * size / dt / 1e6))
    good, dt = timed(300, H + "/")
    print("FIGURE HTTP: %d of 300 requests, %.0f per second" % (good, good / dt))
    good, dt = timed(100, "https://%s:%d/" % (HOST, HTTPS), "--tlsv1.3")
    print("FIGURE HTTPS: %d of 100 requests, each a new handshake, %.1f per second (%.1f ms each)" % (good, good / dt, 1000 * dt / max(good, 1)))
    sh = []
    for _ in range(10):
        o = subprocess.run(["curl", "-sk", "--tlsv1.3", "-o", "/dev/null", "-w", "%{time_connect} %{time_appconnect}", "https://%s:%d/" % (HOST, HTTPS)], capture_output=True, text=True).stdout.split()
        sh.append(float(o[1]) - float(o[0]))
    print("FIGURE TLS handshake: median %.1f ms" % (1000 * sorted(sh)[5]))
sys.exit(0 if ok else 1)
