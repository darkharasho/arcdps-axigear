//! arcdps entry points for axigear. Windows-only modules are gated so
//! `cargo test --workspace` still builds and tests this crate on Linux.

#[cfg(windows)]
#[global_allocator]
static GLOBAL_ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub mod http;
pub mod worker;

#[cfg(windows)]
mod mumble;
#[cfg(windows)]
mod paths;
#[cfg(windows)]
pub mod plugin;
#[cfg(windows)]
mod signals;

#[cfg(windows)]
arcdps::export! {
    name: "axigear",
    sig: 0x6A3E9C51,
    init: plugin::init,
    release: plugin::release,
    combat: plugin::combat,
    imgui: plugin::imgui,
}
