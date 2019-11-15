/*!
This crate provides convenience methods for encoding and decoding numbers in
either [big-endian or little-endian order] on top of asynchronous I/O streams.
It owes everything to the magnificent [`byteorder`] crate. This crate only
provides a shim to [`AsyncRead`] and [`AsyncWrite`].
The organization of the crate mirrors that of `byteorder`. A trait, [`ByteOrder`], specifies
byte conversion methods for each type of number in Rust (sans numbers that have
a platform dependent size like `usize` and `isize`). Two types, [`BigEndian`]
and [`LittleEndian`] implement these methods. Finally, [`AsyncReadBytesExt`] and
[`AsyncWriteBytesExt`] provide convenience methods available to all types that
implement [`Read`] and [`Write`].
An alias, [`NetworkEndian`], for [`BigEndian`] is provided to help improve
code clarity.
An additional alias, [`NativeEndian`], is provided for the endianness of the
local platform. This is convenient when serializing data for use and
conversions are not desired.
# Examples
Read unsigned 16 bit big-endian integers from an [`AsyncRead`] type:
```rust
use std::io::Cursor;
use tokio_byteorder::tokio::{BigEndian, AsyncReadBytesExt};
#[tokio::main]
async fn main() {
    let mut rdr = Cursor::new(vec![2, 5, 3, 0]);
    // Note that we use type parameters to indicate which kind of byte order
    // we want!
    assert_eq!(517, rdr.read_u16::<BigEndian>().await.unwrap());
    assert_eq!(768, rdr.read_u16::<BigEndian>().await.unwrap());
}
```
Write unsigned 16 bit little-endian integers to a [`AsyncWrite`] type:
```rust
use tokio_byteorder::tokio::{LittleEndian, AsyncWriteBytesExt};
#[tokio::main]
async fn main() {
    let mut wtr = vec![];
    wtr.write_u16::<LittleEndian>(517).await.unwrap();
    wtr.write_u16::<LittleEndian>(768).await.unwrap();
    assert_eq!(wtr, vec![5, 2, 0, 3]);
}
```
# Alternatives
Note that as of Rust 1.32, the standard numeric types provide built-in methods
like `to_le_bytes` and `from_le_bytes`, which support some of the same use
cases.
[big-endian or little-endian order]: https://en.wikipedia.org/wiki/Endianness
[`byteorder`]: https://github.com/BurntSushi/byteorder/
[`ByteOrder`]: trait.ByteOrder.html
[`BigEndian`]: enum.BigEndian.html
[`LittleEndian`]: enum.LittleEndian.html
[`AsyncReadBytesExt`]: trait.AsyncReadBytesExt.html
[`AsyncWriteBytesExt`]: trait.AsyncWriteBytesExt.html
[`NetworkEndian`]: type.NetworkEndian.html
[`NativeEndian`]: type.NativeEndian.html
[`AsyncRead`]: https://docs.rs/tokio/0.2.0-alpha.4/tokio/io/trait.AsyncRead.html
[`AsyncWrite`]: https://docs.rs/tokio/0.2.0-alpha.4/tokio/io/trait.AsyncWrite.html
*/

#![deny(missing_docs)]
#![warn(rust_2018_idioms)]

pub use byteorder::{BigEndian, LittleEndian, NativeEndian, NetworkEndian};

macro_rules! reader {
    ($name:ident, $ty:ty, $reader:ident) => {
        reader!($name, $ty, $reader, size_of::<$ty>());
    };
    ($name:ident, $ty:ty, $reader:ident, $bytes:expr) => {
        #[doc(hidden)]
        pub struct $name<R, T> {
            buf: [u8; $bytes],
            read: u8,
            src: R,
            bo: PhantomData<T>,
        }

        impl<R, T> $name<R, T> {
            fn new(r: R) -> Self {
                $name {
                    buf: [0; $bytes],
                    read: 0,
                    src: r,
                    bo: PhantomData,
                }
            }
        }

        impl<R, T> Future for $name<R, T>
        where
            R: io::AsyncRead,
            T: ByteOrder,
        {
            type Output = io::Result<$ty>;
            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                if self.read == $bytes as u8 {
                    return Poll::Ready(Ok(T::$reader(&self.buf[..])));
                }

                // we need this so that we can mutably borrow multiple fields
                // it is safe as long as we never take &mut to src (since it has been pinned)
                // unless it is to place it in a Pin itself like below.
                let mut this = unsafe { self.get_unchecked_mut() };
                let mut src = unsafe { Pin::new_unchecked(&mut this.src) };

                while this.read < $bytes as u8 {
                    this.read += match src
                        .as_mut()
                        .poll_read(cx, &mut this.buf[this.read as usize..])
                    {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Err(e)) => return Poll::Ready(Err(e.into())),
                        Poll::Ready(Ok(0)) => {
                            return Poll::Ready(Err(io::Error::new(
                                io::ErrorKind::UnexpectedEof,
                                "failed to fill whole buffer",
                            )));
                        }
                        Poll::Ready(Ok(n)) => n as u8,
                    };
                }
                Poll::Ready(Ok(T::$reader(&this.buf[..])))
            }
        }
    };
}

macro_rules! reader8 {
    ($name:ident, $ty:ty) => {
        #[doc(hidden)]
        pub struct $name<R>(R);
        impl<R> Future for $name<R>
        where
            R: io::AsyncRead,
        {
            type Output = io::Result<$ty>;
            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let src = unsafe { self.map_unchecked_mut(|t| &mut t.0) };
                let mut buf = [0; 1];
                match src.poll_read(cx, &mut buf[..]) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(Err(e)) => Poll::Ready(Err(e.into())),
                    Poll::Ready(Ok(0)) => Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "failed to fill whole buffer",
                    ))),
                    Poll::Ready(Ok(1)) => Poll::Ready(Ok(buf[0] as $ty)),
                    Poll::Ready(Ok(_)) => unreachable!(),
                }
            }
        }
    };
}

macro_rules! read_impl {
    (
        $(#[$outer:meta])*
        fn $name:ident(&mut self) -> $($fut:tt)*
    ) => {
        $(#[$outer])*
        #[inline]
        fn $name<'a, T: ByteOrder>(&'a mut self) -> $($fut)*<&'a mut Self, T> where Self: Unpin {
            $($fut)*::new(self)
        }
    }
}

macro_rules! writer {
    ($name:ident, $ty:ty, $writer:ident) => {
        writer!($name, $ty, $writer, size_of::<$ty>());
    };
    ($name:ident, $ty:ty, $writer:ident, $bytes:expr) => {
        #[doc(hidden)]
        pub struct $name<W> {
            buf: [u8; $bytes],
            written: u8,
            dst: W,
        }

        impl<W> $name<W> {
            fn new<T: ByteOrder>(w: W, value: $ty) -> Self {
                let mut writer = $name {
                    buf: [0; $bytes],
                    written: 0,
                    dst: w,
                };
                T::$writer(&mut writer.buf[..], value);
                writer
            }
        }

        impl<W> Future for $name<W>
        where
            W: io::AsyncWrite,
        {
            type Output = io::Result<()>;
            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                if self.written == $bytes as u8 {
                    return Poll::Ready(Ok(()));
                }

                // we need this so that we can mutably borrow multiple fields
                // it is safe as long as we never take &mut to dst (since it has been pinned)
                // unless it is to place it in a Pin itself like below.
                let mut this = unsafe { self.get_unchecked_mut() };
                let mut dst = unsafe { Pin::new_unchecked(&mut this.dst) };

                while this.written < $bytes as u8 {
                    this.written += match dst
                        .as_mut()
                        .poll_write(cx, &this.buf[this.written as usize..])
                    {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Err(e)) => return Poll::Ready(Err(e.into())),
                        Poll::Ready(Ok(n)) => n as u8,
                    };
                }
                Poll::Ready(Ok(()))
            }
        }
    };
}

macro_rules! writer8 {
    ($name:ident, $ty:ty) => {
        #[doc(hidden)]
        pub struct $name<W>(W, $ty);
        impl<W> Future for $name<W>
        where
            W: io::AsyncWrite,
        {
            type Output = io::Result<()>;
            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let this = unsafe { self.get_unchecked_mut() };
                let dst = unsafe { Pin::new_unchecked(&mut this.0) };
                let buf = [this.1 as u8];
                match dst.poll_write(cx, &buf[..]) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(Err(e)) => Poll::Ready(Err(e.into())),
                    Poll::Ready(Ok(0)) => Poll::Pending,
                    Poll::Ready(Ok(1)) => Poll::Ready(Ok(())),
                    Poll::Ready(Ok(_)) => unreachable!(),
                }
            }
        }
    };
}

macro_rules! write_impl {
    (
        $(#[$outer:meta])*
        fn $name:ident(&mut self, n: $ty:ty) -> $($fut:tt)*
    ) => {
        $(#[$outer])*
        #[inline]
        fn $name<'a, T: ByteOrder>(&'a mut self, n: $ty) -> $($fut)*<&'a mut Self> where Self: Unpin {
            $($fut)*::new::<T>(self, n)
        }
    }
}

/// Tokio traits feature gate
#[cfg(feature = "tokio-traits")]
pub mod tokio {
    /// Tokio io implementation
    pub mod io_tokio;
    pub use io_tokio::{
        AsyncReadBytesExt, AsyncWriteBytesExt, BigEndian, LittleEndian, NativeEndian, NetworkEndian,
    };
}

/// Futures io traits feature gate
#[cfg(feature = "futures-traits")]
pub mod futures {
    /// Futures io implementation
    pub mod io_futures;
    pub use io_futures::{
        AsyncReadBytesExt, AsyncWriteBytesExt, BigEndian, LittleEndian, NativeEndian, NetworkEndian,
    };
}
/// Prelude for convinience
pub mod prelude {
    #[cfg(features = "tokio-traits")]
    pub use tokio::io_tokio::{AsyncReadBytesExt, AsyncWriteBytesExt, BigEndian, LittleEndian, NativeEndian, NetworkEndian};
    #[cfg(features = "futures-traits")]
    pub use tokio::io_futures::{AsyncReadBytesExt, AsyncWriteBytesExt, BigEndian, LittleEndian, NativeEndian, NetworkEndian};
}
