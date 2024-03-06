use futures::Future;

cfg_if::cfg_if! {
    if #[cfg(all(feature = "tokio-comp", not(feature = "async-std-comp")))] {
        pub use tokio::task::JoinHandle;
        pub fn spawn(f: impl Future<Output = ()> + Send + 'static) -> JoinHandle<()> {
            tokio::spawn(f)
        }

        pub fn abort(handle: &JoinHandle<()>) {
            handle.abort();
        }

        pub struct Instant(tokio::time::Instant);
        impl Instant {
            pub fn now() -> Instant {
                Instant(tokio::time::Instant::now())
            }

            pub fn elapsed(&self) -> std::time::Duration {
                self.0.elapsed()
            }
        }
    } else if #[cfg(feature = "async-std-comp")] {
        pub use async_std::task::JoinHandle;
        pub fn spawn(f: impl Future<Output = ()> + Send + 'static) -> JoinHandle<()> {
            async_std::task::spawn(f)
        }

        pub fn abort(_handle: &JoinHandle<()>) {}

        pub struct Instant(std::time::Instant);
        impl Instant {
            pub fn now() -> Instant {
                Instant(std::time::Instant::now())
            }

            pub fn elapsed(&self) -> std::time::Duration {
                self.0.elapsed()
            }
        }
    } else {
        compile_error!("tokio-comp or async-std-comp features required");
    }
}
