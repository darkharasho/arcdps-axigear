use crate::util::Share;
use std::{
    ffi::c_void,
    sync::{
        OnceLock,
        atomic::{AtomicU32, Ordering},
    },
};
use windows::{
    Win32::Graphics::{Direct3D11::ID3D11Device, Dxgi::IDXGISwapChain},
    core::{Interface, InterfaceRef},
};

/// Current DirectX version.
static D3D_VERSION: AtomicU32 = AtomicU32::new(0);

/// Returns the current DirectX version.
///
/// `11` for DirectX 11 and `9` for legacy DirectX 9 mode.
#[inline]
pub fn d3d_version() -> u32 {
    D3D_VERSION.load(Ordering::Relaxed)
}

/// DirectX 11 swap chain.
static DXGI_SWAP_CHAIN: OnceLock<Share<InterfaceRef<'static, IDXGISwapChain>>> = OnceLock::new();

/// Returns the DirectX swap chain, if available.
#[inline]
pub fn dxgi_swap_chain() -> Option<IDXGISwapChain> {
    DXGI_SWAP_CHAIN
        .get()
        .map(|swap_chain| (*unsafe { swap_chain.get() }).to_owned())
}

/// Returns the DirectX 11 device, if available.
#[inline]
pub fn d3d11_device() -> Option<ID3D11Device> {
    let swap_chain = dxgi_swap_chain()?;
    unsafe { swap_chain.GetDevice() }.ok()
}

/// Initializes DirectX information.
///
/// *Vendored patch:* the upstream binding only stored the swap chain
/// when `d3d_version == 11`, but the actual arcdps `get_init_addr` ABI
/// passes the *imgui version* (e.g. 19270) in the 7th arg, not the d3d
/// version. The check failed for every plugin, leaving the swap chain
/// unset and `d3d11_device()` permanently `None`. Modern GW2 is d3d11
/// only — accept any non-null `id3d` and stop gating on the misnamed
/// version field.
pub unsafe fn init_dxgi(id3d: *mut c_void, d3d_version: u32) {
    D3D_VERSION.store(d3d_version, Ordering::Relaxed);
    if !id3d.is_null() {
        let swap_chain =
            unsafe { IDXGISwapChain::from_raw_borrowed(&id3d) }.expect("invalid swap chain");
        DXGI_SWAP_CHAIN
            .get_or_init(|| unsafe { Share::new(InterfaceRef::from_interface(swap_chain)) });
    }
}
