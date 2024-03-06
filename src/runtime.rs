use futures::Future;

#[cfg(feature = "tokio-comp")]
pub use tokio::task::JoinHandle;

cfg_if::cfg_if! {
    if #[cfg(all(feature = "tokio-comp", not(feature = "async-std-comp")))] {
        pub fn spawn(f: impl Future<Output = ()> + Send + 'static) -> JoinHandle<()> {
            tokio::spawn(f)
        }
    } else if #[cfg(all(feature = "async-std-comp", not(feature = "tokio-comp")))] {
        pub fn spawn(f: impl Future<Output = ()> + Send + 'static) -> JoinHandle<()> {
            async_std::task::spawn(f)
        }
    } else {
        compile_error!("tokio-comp or async-std-comp features required");
    }
}
