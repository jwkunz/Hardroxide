//! An HTTP/1.1 server over the stack's TCP sockets: parse a request, let a handler fill in a response, send it, close.
//!
//! One request per connection (`Connection: close`); a handful of connections are served at once, one per socket given to the
//! [`Server`]. Responses may be larger than the TCP buffer: the body is either fixed bytes, written by the handler into a scratch buffer,
//! or produced in pieces by a function as the connection has room.

use crate::tcp::State;
use crate::{Device, Stack};
use core::fmt::Write;

/// Why a request could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// More bytes are needed.
    Incomplete,
    /// Not an HTTP/1.x request.
    Bad,
}

/// A parsed request. It borrows the connection's buffer.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    pub method: &'a str,
    /// The path without the query string.
    pub path: &'a str,
    /// The query string without the `?` (empty if none).
    pub query: &'a str,
    headers: &'a str,
    pub body: &'a [u8],
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Read a request from `buf`. Returns `Incomplete` until the headers and `Content-Length` bytes of body are all there.
pub fn parse(buf: &[u8]) -> Result<Request<'_>, ParseError> {
    let end = find(buf, b"\r\n\r\n").ok_or(ParseError::Incomplete)?;
    let head = core::str::from_utf8(&buf[..end]).map_err(|_| ParseError::Bad)?;
    let (line, headers) = head.split_once("\r\n").unwrap_or((head, ""));
    let mut parts = line.split(' ');
    let (method, target, version) = (
        parts.next().ok_or(ParseError::Bad)?,
        parts.next().ok_or(ParseError::Bad)?,
        parts.next().ok_or(ParseError::Bad)?,
    );
    if !version.starts_with("HTTP/1.") || method.is_empty() || !target.starts_with('/') {
        return Err(ParseError::Bad);
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut req = Request {
        method,
        path,
        query,
        headers,
        body: &[],
    };
    let want = match req.header("Content-Length") {
        Some(v) => v.trim().parse::<usize>().map_err(|_| ParseError::Bad)?,
        None => 0,
    };
    let body = &buf[end + 4..];
    if body.len() < want {
        return Err(ParseError::Incomplete);
    }
    req.body = &body[..want];
    Ok(req)
}

impl<'a> Request<'a> {
    /// The value of a header (the name is matched without regard to case).
    pub fn header(&self, name: &str) -> Option<&'a str> {
        self.headers.split("\r\n").find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim().eq_ignore_ascii_case(name).then_some(v.trim())
        })
    }

    /// The raw value of `key` in the query string or, failing that, in a form body (`a=1&b=2`), still percent-encoded.
    pub fn param(&self, key: &str) -> Option<&'a str> {
        let body = core::str::from_utf8(self.body).ok().filter(|_| {
            self.header("Content-Type")
                .is_some_and(|t| t.starts_with("application/x-www-form-urlencoded"))
        });
        for source in [Some(self.query), body].into_iter().flatten() {
            for pair in source.split('&') {
                let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                if k == key {
                    return Some(v);
                }
            }
        }
        None
    }
}

/// Decode `%XX` and `+` into `out`; returns the length (stops when `out` is full).
pub fn percent_decode(s: &str, out: &mut [u8]) -> usize {
    let b = s.as_bytes();
    let (mut i, mut n) = (0, 0);
    let hex = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    while i < b.len() && n < out.len() {
        out[n] = match b[i] {
            b'+' => b' ',
            b'%' if i + 2 < b.len() && hex(b[i + 1]).is_some() && hex(b[i + 2]).is_some() => {
                let v = hex(b[i + 1]).unwrap() << 4 | hex(b[i + 2]).unwrap();
                i += 2;
                v
            }
            c => c,
        };
        i += 1;
        n += 1;
    }
    n
}

/// A body made in pieces: `producer(arg, offset, out)` fills `out` with the body's bytes from `offset` and returns how many.
pub type Generator = fn(arg: u32, offset: usize, out: &mut [u8]) -> usize;

#[derive(Clone, Copy)]
enum Body {
    Empty,
    Static(&'static [u8]),
    /// `len` bytes at the start of the connection's scratch buffer.
    Scratch(usize),
    Generated {
        len: usize,
        producer: Generator,
        arg: u32,
    },
}

/// The response a handler fills in.
pub struct Response<'a> {
    status: u16,
    content_type: &'static str,
    body: Body,
    scratch: &'a mut [u8],
}

struct SliceWriter<'a> {
    buf: &'a mut [u8],
    len: usize,
}

impl Write for SliceWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let n = s.len().min(self.buf.len() - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        if n < s.len() {
            Err(core::fmt::Error)
        } else {
            Ok(())
        }
    }
}

impl<'a> Response<'a> {
    /// A response with a status, a content type and a body written with `format_args!` (cut off if it does not fit the scratch buffer).
    pub fn text(&mut self, status: u16, content_type: &'static str, args: core::fmt::Arguments) {
        let mut w = SliceWriter {
            buf: &mut *self.scratch,
            len: 0,
        };
        let _ = w.write_fmt(args);
        let len = w.len;
        self.status = status;
        self.content_type = content_type;
        self.body = Body::Scratch(len);
    }
    /// A fixed body.
    pub fn bytes(&mut self, status: u16, content_type: &'static str, body: &'static [u8]) {
        self.status = status;
        self.content_type = content_type;
        self.body = Body::Static(body);
    }
    /// A body of `len` bytes produced by `producer` as the connection has room (for pictures and other large things).
    pub fn generated(
        &mut self,
        status: u16,
        content_type: &'static str,
        len: usize,
        producer: Generator,
        arg: u32,
    ) {
        self.status = status;
        self.content_type = content_type;
        self.body = Body::Generated { len, producer, arg };
    }
    /// No body.
    pub fn empty(&mut self, status: u16) {
        self.status = status;
        self.body = Body::Empty;
    }
    /// The scratch buffer, for building a body by hand before calling [`Response::scratch_body`].
    pub fn scratch(&mut self) -> &mut [u8] {
        self.scratch
    }
    /// Use the first `len` bytes of the scratch buffer as the body.
    pub fn scratch_body(&mut self, status: u16, content_type: &'static str, len: usize) {
        self.status = status;
        self.content_type = content_type;
        self.body = Body::Scratch(len.min(self.scratch.len()));
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        302 => "Found",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        _ => "Status",
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Reading,
    Sending,
    Closing,
}

struct Conn<const REQ: usize, const SCRATCH: usize> {
    phase: Phase,
    req: [u8; REQ],
    req_len: usize,
    head: [u8; 192],
    head_len: usize,
    head_sent: usize,
    body: Body,
    body_sent: usize,
    scratch: [u8; SCRATCH],
}

impl<const REQ: usize, const SCRATCH: usize> Conn<REQ, SCRATCH> {
    const fn new() -> Self {
        Conn {
            phase: Phase::Reading,
            req: [0; REQ],
            req_len: 0,
            head: [0; 192],
            head_len: 0,
            head_sent: 0,
            body: Body::Empty,
            body_sent: 0,
            scratch: [0; SCRATCH],
        }
    }
    fn reset(&mut self) {
        self.phase = Phase::Reading;
        self.req_len = 0;
        self.head_len = 0;
        self.head_sent = 0;
        self.body = Body::Empty;
        self.body_sent = 0;
    }
}

/// An HTTP server on `port` using `CONNS` of the stack's TCP sockets (from `first_socket`), with `REQ` bytes of request buffer and
/// `SCRATCH` bytes of response scratch space per connection.
pub struct Server<const CONNS: usize, const REQ: usize, const SCRATCH: usize> {
    port: u16,
    first_socket: usize,
    conns: [Conn<REQ, SCRATCH>; CONNS],
    /// Requests answered.
    pub served: u32,
}

impl<const CONNS: usize, const REQ: usize, const SCRATCH: usize> Server<CONNS, REQ, SCRATCH> {
    pub const fn new(port: u16, first_socket: usize) -> Self {
        Server {
            port,
            first_socket,
            conns: [const { Conn::new() }; CONNS],
            served: 0,
        }
    }

    /// Advance every connection: accept, read, call `handler` for each complete request, send responses.
    pub fn poll<D: Device, const T: usize, const U: usize, const BUF: usize>(
        &mut self,
        stack: &mut Stack<D, T, U, BUF>,
        mut handler: impl FnMut(&Request, &mut Response),
    ) {
        for k in 0..CONNS {
            let conn = &mut self.conns[k];
            let sock = stack.tcp(self.first_socket + k);
            match sock.state() {
                // A finished connection waits in TIME-WAIT in case its last acknowledgement is lost; a server short of sockets
                // cannot afford that, and a retransmitted FIN that finds the socket listening again just draws a reset.
                State::Closed | State::TimeWait => {
                    sock.listen(self.port);
                    conn.reset();
                    continue;
                }
                State::Listen | State::SynReceived | State::SynSent => continue,
                _ => {}
            }
            if sock.was_reset() {
                sock.abort();
                continue;
            }
            if conn.phase == Phase::Reading {
                let n = sock.recv(&mut conn.req[conn.req_len..]);
                conn.req_len += n;
                let status = match parse(&conn.req[..conn.req_len]) {
                    Ok(req) => {
                        let mut res = Response {
                            status: 200,
                            content_type: "text/plain",
                            body: Body::Empty,
                            scratch: &mut conn.scratch,
                        };
                        handler(&req, &mut res);
                        self.served += 1;
                        let (status, ct, body) = (res.status, res.content_type, res.body);
                        conn.body = body;
                        Some((status, ct))
                    }
                    Err(ParseError::Bad) => {
                        conn.body = Body::Static(b"bad request\n");
                        Some((400, "text/plain"))
                    }
                    Err(ParseError::Incomplete) if conn.req_len == REQ => {
                        conn.body = Body::Static(b"request too large\n");
                        Some((413, "text/plain"))
                    }
                    Err(ParseError::Incomplete) => {
                        if sock.peer_closed() {
                            sock.abort();
                        }
                        None
                    }
                };
                if let Some((status, ct)) = status {
                    let len = match conn.body {
                        Body::Empty => 0,
                        Body::Static(b) => b.len(),
                        Body::Scratch(n) => n,
                        Body::Generated { len, .. } => len,
                    };
                    let mut w = SliceWriter {
                        buf: &mut conn.head,
                        len: 0,
                    };
                    let _ = write!(
                        w,
                        "HTTP/1.1 {status} {}\r\nContent-Type: {ct}\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n",
                        reason(status)
                    );
                    conn.head_len = w.len;
                    conn.head_sent = 0;
                    conn.body_sent = 0;
                    conn.phase = Phase::Sending;
                }
            }
            if conn.phase == Phase::Sending {
                if conn.head_sent < conn.head_len {
                    conn.head_sent += sock.send(&conn.head[conn.head_sent..conn.head_len]);
                }
                if conn.head_sent == conn.head_len {
                    match conn.body {
                        Body::Empty => conn.body_sent = usize::MAX,
                        Body::Static(b) => conn.body_sent += sock.send(&b[conn.body_sent..]),
                        Body::Scratch(n) => {
                            conn.body_sent += sock.send(&conn.scratch[conn.body_sent..n])
                        }
                        Body::Generated { len, producer, arg } => {
                            let mut piece = [0u8; 256];
                            while conn.body_sent < len {
                                let room =
                                    sock.send_space().min(piece.len()).min(len - conn.body_sent);
                                if room == 0 {
                                    break;
                                }
                                let got = producer(arg, conn.body_sent, &mut piece[..room]);
                                if got == 0 {
                                    // The generator ran dry early: stop here rather than hang.
                                    conn.body_sent = len;
                                    break;
                                }
                                conn.body_sent += sock.send(&piece[..got]);
                            }
                        }
                    }
                    let total = match conn.body {
                        Body::Empty => 0,
                        Body::Static(b) => b.len(),
                        Body::Scratch(n) => n,
                        Body::Generated { len, .. } => len,
                    };
                    if conn.body_sent == usize::MAX || conn.body_sent >= total {
                        sock.close();
                        conn.phase = Phase::Closing;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_parsed_and_incomplete_ones_wait() {
        let raw = b"GET /led?n=3&on=1 HTTP/1.1\r\nHost: board\r\nUser-Agent: curl/8\r\n\r\n";
        let r = parse(raw).unwrap();
        assert_eq!((r.method, r.path, r.query), ("GET", "/led", "n=3&on=1"));
        assert_eq!(r.header("host"), Some("board"));
        assert_eq!(r.param("on"), Some("1"));
        assert_eq!(r.param("x"), None);
        assert_eq!(parse(&raw[..20]).unwrap_err(), ParseError::Incomplete);

        let post = b"POST /set HTTP/1.1\r\nContent-Length: 11\r\nContent-Type: application/x-www-form-urlencoded\r\n\r\nname=a+b%21";
        let r = parse(post).unwrap();
        assert_eq!(r.body, b"name=a+b%21");
        assert_eq!(r.param("name"), Some("a+b%21"));
        let mut out = [0u8; 16];
        let n = percent_decode("a+b%21", &mut out);
        assert_eq!(&out[..n], b"a b!");
        // A body that has not all arrived.
        assert_eq!(
            parse(&post[..post.len() - 3]).unwrap_err(),
            ParseError::Incomplete
        );
        assert_eq!(
            parse(b"BREW /pot COFFEE/1.0\r\n\r\n").unwrap_err(),
            ParseError::Bad
        );
        assert_eq!(
            parse(b"GET nothing HTTP/1.1\r\n\r\n").unwrap_err(),
            ParseError::Bad
        );
    }
}
