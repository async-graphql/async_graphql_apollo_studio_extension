#![allow(rustdoc::all)]
#![allow(clippy::all)]

pub mod reports {
    include!(concat!(env!("OUT_DIR"), concat!("/proto/reports.rs")));
}
