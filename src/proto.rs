#![allow(rustdoc::all)]
#![allow(clippy::all)]

cfg_if::cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {
        pub mod report {
            tonic::include_proto!("report");
        }
    } else {
        pub mod report {
            include!(concat!(env!("OUT_DIR"), "/report.rs"));
        }
    }
}
