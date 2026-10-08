//! arcdps entry points for axigear. Everything Windows-only is gated so
//! `cargo test --workspace` still builds this crate on Linux.

#[cfg(windows)]
#[global_allocator]
static GLOBAL_ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(windows)]
mod plugin;

#[cfg(windows)]
arcdps::export! {
    name: "axigear",
    sig: 0x6A3E9C51,
    init: plugin::init,
    release: plugin::release,
}
