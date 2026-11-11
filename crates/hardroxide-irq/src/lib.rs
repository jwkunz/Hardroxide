//! Interrupt waiting through a Linux UIO device (`/dev/uioN`).
//!
//! UIO protocol: writing a 32-bit `1` unmasks the interrupt; a blocking
//! 4-byte read returns the cumulative event count once it fires (the kernel
//! masks the line while it is pending). Device-tree/UIO setup per IP and the
//! behaviour on the stock PYNQ image are UNVERIFIED (docs/08).

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum IrqError {
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub struct UioIrq {
    file: File,
    path: PathBuf,
}

impl UioIrq {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, IrqError> {
        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|source| IrqError::Io { path: path.clone(), source })?;
        Ok(Self { file, path })
    }

    /// Wrap an already-open descriptor (tests use a pipe).
    pub fn from_file(file: File, path: impl Into<PathBuf>) -> Self {
        Self { file, path: path.into() }
    }

    fn io(&self, source: std::io::Error) -> IrqError {
        IrqError::Io { path: self.path.clone(), source }
    }

    /// Unmask the interrupt in the UIO core.
    pub fn enable(&self) -> Result<(), IrqError> {
        (&self.file).write_all(&1u32.to_ne_bytes()).map_err(|e| self.io(e))
    }

    /// Unmask, then wait for the next interrupt. Returns the cumulative
    /// event count, or `None` on timeout. The caller must acknowledge the
    /// device (e.g. `HlsIp::ack_interrupts`) before waiting again.
    pub fn wait(&self, timeout: Option<Duration>) -> Result<Option<u32>, IrqError> {
        self.enable()?;
        self.wait_event(timeout)
    }

    /// Wait without re-enabling first.
    pub fn wait_event(&self, timeout: Option<Duration>) -> Result<Option<u32>, IrqError> {
        let mut pfd = libc::pollfd { fd: self.file.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        let ms = timeout.map_or(-1, |t| t.as_millis().min(i32::MAX as u128) as i32);
        loop {
            // SAFETY: `pfd` is a valid pollfd and we pass exactly one.
            let n = unsafe { libc::poll(&mut pfd, 1, ms) };
            match n {
                0 => return Ok(None),
                n if n > 0 => break,
                _ => {
                    let e = std::io::Error::last_os_error();
                    if e.kind() != std::io::ErrorKind::Interrupted {
                        return Err(self.io(e));
                    }
                }
            }
        }
        let mut buf = [0u8; 4];
        (&self.file).read_exact(&mut buf).map_err(|e| self.io(e))?;
        Ok(Some(u32::from_ne_bytes(buf)))
    }
}

#[cfg(feature = "async")]
mod async_impl {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Poll, Waker};

    struct Shared {
        result: Option<Result<Option<u32>, IrqError>>,
        waker: Option<Waker>,
    }

    /// Future returned by [`UioIrq::wait_async`].
    pub struct WaitFuture(Arc<Mutex<Shared>>);

    impl UioIrq {
        /// Async [`UioIrq::wait`]: a helper thread blocks in `poll` and wakes
        /// the task. Works under any executor.
        pub fn wait_async(self: &Arc<Self>, timeout: Option<Duration>) -> WaitFuture {
            let shared = Arc::new(Mutex::new(Shared { result: None, waker: None }));
            let (me, sh) = (self.clone(), shared.clone());
            std::thread::spawn(move || {
                let r = me.wait(timeout);
                let mut g = sh.lock().unwrap();
                g.result = Some(r);
                if let Some(w) = g.waker.take() {
                    w.wake();
                }
            });
            WaitFuture(shared)
        }
    }

    impl Future for WaitFuture {
        type Output = Result<Option<u32>, IrqError>;
        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let mut g = self.0.lock().unwrap();
            match g.result.take() {
                Some(r) => Poll::Ready(r),
                None => {
                    g.waker = Some(cx.waker().clone());
                    Poll::Pending
                }
            }
        }
    }
}

#[cfg(feature = "async")]
pub use async_impl::WaitFuture;

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::FromRawFd;

    /// A pipe pair: `(read end as the fake uio, write end for the test)`.
    /// The fake device also needs to accept `enable()` writes, so use a
    /// socketpair (bidirectional) for the UIO side.
    fn pair() -> (File, File) {
        let mut fds = [0; 2];
        // SAFETY: valid out-array of 2 ints.
        assert_eq!(unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()) }, 0);
        // SAFETY: fresh descriptors we own.
        unsafe { (File::from_raw_fd(fds[0]), File::from_raw_fd(fds[1])) }
    }

    #[test]
    fn times_out_then_receives_count() {
        let (uio, mut dev) = pair();
        let irq = UioIrq::from_file(uio, "fake");
        assert_eq!(irq.wait(Some(Duration::from_millis(20))).unwrap(), None);
        dev.write_all(&7u32.to_ne_bytes()).unwrap();
        // enable() wrote 1u32 twice by now; the device side saw them.
        assert_eq!(irq.wait(Some(Duration::from_millis(200))).unwrap(), Some(7));
        let mut seen = [0u8; 8];
        dev.read_exact(&mut seen).unwrap();
        assert_eq!(seen, [1, 0, 0, 0, 1, 0, 0, 0]);
    }

    #[cfg(feature = "async")]
    #[test]
    fn async_wait_completes() {
        use std::sync::Arc;
        use std::task::{Context, Poll, Wake, Waker};
        struct Flag(std::sync::atomic::AtomicBool, std::thread::Thread);
        impl Wake for Flag {
            fn wake(self: Arc<Self>) {
                self.0.store(true, std::sync::atomic::Ordering::SeqCst);
                self.1.unpark();
            }
        }
        let (uio, mut dev) = pair();
        let irq = Arc::new(UioIrq::from_file(uio, "fake"));
        let mut fut = Box::pin(irq.wait_async(Some(Duration::from_secs(2))));
        let flag = Arc::new(Flag(false.into(), std::thread::current()));
        let waker = Waker::from(flag.clone());
        let mut cx = Context::from_waker(&waker);
        assert!(fut.as_mut().poll(&mut cx).is_pending());
        dev.write_all(&3u32.to_ne_bytes()).unwrap();
        while !flag.0.load(std::sync::atomic::Ordering::SeqCst) {
            std::thread::park_timeout(Duration::from_millis(50));
        }
        use std::future::Future;
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(r) => assert_eq!(r.unwrap(), Some(3)),
            Poll::Pending => panic!("not ready after wake"),
        }
    }
}
