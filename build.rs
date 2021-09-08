#[cfg(not(any(feature = "pio", feature = "native")))]
compile_error!("One of the features `pio` or `native` must be selected.");

#[cfg(all(feature = "pio", feature = "native"))]
compile_error!("The features `pio` and `native` are mutually exclusive. Only one of them can be selected at a time.");

#[cfg(any(feature = "pio", feature = "native"))]
#[cfg_attr(feature = "pio", path = "build_pio.rs")]
#[cfg_attr(feature = "native", path = "build_native.rs")]
mod build_impl;

fn main() -> anyhow::Result<()> {
    build_impl::main()
}