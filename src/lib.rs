#[cfg(feature = "tokio-traits")]
pub mod tokio {
    pub mod io_tokio;
    pub use io_tokio::{AsyncReadBytesExt, AsyncWriteBytesExt, BigEndian, LittleEndian, NativeEndian, NetworkEndian};
}

#[cfg(feature = "futures-traits")]
pub mod futures {
    pub mod io_futures;
    pub use io_futures::{AsyncReadBytesExt, AsyncWriteBytesExt, BigEndian, LittleEndian, NativeEndian, NetworkEndian};
}
