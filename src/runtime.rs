use futures::Future;

#[cfg(feature = "async-std-comp")]
pub use async_std::channel::{bounded as channel, Receiver, Sender};
#[cfg(feature = "async-std-comp")]
pub use async_std::sync::RwLock;
#[cfg(feature = "tokio-comp")]
pub use tokio::sync::mpsc::{channel, Receiver, Sender};
#[cfg(feature = "tokio-comp")]
pub use tokio::sync::RwLock;

// From https://github.com/mitsuhiko/redis-rs/blob/99a97e8876c99df5a0cf5536fbff21b5d9cae14c/src/aio.rs
#[derive(Clone, Debug)]
pub(crate) enum Runtime {
    #[cfg(feature = "tokio-comp")]
    Tokio,
    #[cfg(feature = "async-std-comp")]
    AsyncStd,
}

impl Runtime {
    pub fn locate() -> Self {
        #[cfg(all(feature = "tokio-comp", not(feature = "async-std-comp")))]
        {
            Runtime::Tokio
        }

        #[cfg(all(not(feature = "tokio-comp"), feature = "async-std-comp"))]
        {
            Runtime::AsyncStd
        }

        #[cfg(all(feature = "tokio-comp", feature = "async-std-comp"))]
        {
            if ::tokio::runtime::Handle::try_current().is_ok() {
                Runtime::Tokio
            } else {
                Runtime::AsyncStd
            }
        }

        #[cfg(all(not(feature = "tokio-comp"), not(feature = "async-std-comp")))]
        {
            compile_error!("tokio-comp or async-std-comp features required")
        }
    }

    #[allow(dead_code)]
    pub fn spawn(&self, f: impl Future<Output = ()> + Send + 'static) {
        match self {
            #[cfg(feature = "tokio-comp")]
            Runtime::Tokio => tokio::spawn(f),
            #[cfg(feature = "async-std-comp")]
            Runtime::AsyncStd => async_std::task::spawn(f),
        };
    }
}
