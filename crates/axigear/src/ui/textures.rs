#![cfg(windows)]
//! GPU icon texture cache, ported from arcdps-axipulse (`ui/icons.rs`).
//!
//! `get(url)` kicks off a background disk-cache read / HTTP fetch + decode on
//! a single worker thread and returns `None` until ready; `drain_pending`
//! (imgui thread, once per frame) uploads finished images as D3D11 SRVs, at
//! most `MAX_UPLOADS_PER_FRAME` per frame because creating many textures in a
//! single frame has crashed the host under Wine. Only GW2 hosts are fetched
//! (see `texture_rules`). SRVs live for the process lifetime.

use std::collections::HashMap;
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::thread;

use arcdps::imgui::TextureId;
use once_cell::sync::Lazy;
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11ShaderResourceView, ID3D11Texture2D, D3D11_BIND_SHADER_RESOURCE,
    D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC};

use super::texture_rules;

#[derive(Clone, Copy)]
pub struct IconHandle {
    pub tex: TextureId,
    pub aspect: f32,
}

enum State {
    Pending,
    Failed,
    Ready { ptr: usize, aspect: f32 },
}

struct Cache {
    by_key: HashMap<String, State>,
    /// Kept alive for the process lifetime; ImGui holds raw pointers into these.
    _srvs: Vec<ID3D11ShaderResourceView>,
}

unsafe impl Send for Cache {}
unsafe impl Sync for Cache {}

static CACHE: Lazy<Mutex<Cache>> = Lazy::new(|| Mutex::new(Cache { by_key: HashMap::new(), _srvs: Vec::new() }));

/// Decoded RGBA8 pixels; decoding happens on the worker so the imgui thread
/// only does the cheap D3D11 upload.
struct Decoded {
    w: u32,
    h: u32,
    aspect: f32,
    rgba: Vec<u8>,
}

type DownloadResult = (String, Result<Decoded, String>);
type DownloadRequest = (String, PathBuf);

fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?.to_rgba8();
    let (w, h) = (img.width(), img.height());
    let aspect = if h > 0 { w as f32 / h as f32 } else { 1.0 };
    Ok(Decoded { w, h, aspect, rgba: img.into_raw() })
}

struct Chan {
    rx: Mutex<Receiver<DownloadResult>>,
    req_tx: Sender<DownloadRequest>,
}

/// Upper bound on D3D11 uploads per imgui frame (Wine stability).
const MAX_UPLOADS_PER_FRAME: usize = 4;

fn fetch(url: &str, path: &PathBuf) -> Result<Decoded, String> {
    if path.exists() {
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(d) = decode(&bytes) {
                return Ok(d);
            }
        }
        // Corrupt or unreadable cache entry: drop it and re-download.
        let _ = std::fs::remove_file(path);
    }
    let resp = ureq::get(url).timeout(std::time::Duration::from_secs(20)).call().map_err(|e| e.to_string())?;
    let mut bytes: Vec<u8> = Vec::new();
    std::io::copy(&mut resp.into_reader(), &mut bytes).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut tmp = path.clone().into_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    if std::fs::write(&tmp, &bytes).is_ok() && std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    decode(&bytes)
}

static CHAN: Lazy<Chan> = Lazy::new(|| {
    let (result_tx, rx) = mpsc::channel::<DownloadResult>();
    let (req_tx, req_rx) = mpsc::channel::<DownloadRequest>();
    // One worker runs all fetches serially (no thread-per-icon fan-out).
    thread::Builder::new()
        .name("axigear-icon-worker".into())
        .spawn(move || {
            for (url, path) in req_rx {
                let r = fetch(&url, &path);
                let _ = result_tx.send((url, r));
            }
        })
        .ok();
    Chan { rx: Mutex::new(rx), req_tx }
});

/// Look up an icon by URL. `Some` once uploaded; otherwise (and on first
/// sight) starts the fetch and returns `None` so callers can use a fallback.
/// Disallowed URLs are never cached or fetched.
pub fn get(url: &str) -> Option<IconHandle> {
    if !texture_rules::allowed(url) {
        return None;
    }
    let mut c = CACHE.lock().ok()?;
    if let Some(state) = c.by_key.get(url) {
        return match state {
            State::Ready { ptr, aspect } => Some(IconHandle { tex: TextureId::new(*ptr), aspect: *aspect }),
            _ => None,
        };
    }
    c.by_key.insert(url.to_string(), State::Pending);
    drop(c);
    let path = crate::paths::data_dir().join("icons").join(texture_rules::cache_name(url));
    let _ = CHAN.req_tx.send((url.to_string(), path));
    None
}

/// Upload completed fetches. Imgui thread only (the sole safe place to touch
/// the D3D11 device). No device yet means icons stay on text tiles.
pub fn drain_pending() {
    let Some(device) = arcdps::d3d11_device() else { return };
    let Ok(rx) = CHAN.rx.lock() else { return };
    let mut uploaded = 0usize;
    while uploaded < MAX_UPLOADS_PER_FRAME {
        let Ok((url, result)) = rx.try_recv() else { break };
        uploaded += 1;
        let new_state = match result {
            Ok(d) => match unsafe { create_srv(&device, d.w, d.h, &d.rgba) } {
                Ok(srv) => {
                    let ptr = srv.as_raw() as usize;
                    match CACHE.lock() {
                        Ok(mut c) => {
                            c._srvs.push(srv);
                            State::Ready { ptr, aspect: d.aspect }
                        }
                        Err(_) => continue,
                    }
                }
                Err(e) => {
                    log::warn!("axigear icon: {url}: {e}");
                    State::Failed
                }
            },
            Err(e) => {
                log::warn!("axigear icon: {url}: {e}");
                State::Failed
            }
        };
        if let Ok(mut c) = CACHE.lock() {
            c.by_key.insert(url, new_state);
        }
    }
}

/// GPU-only: texture + SRV from pre-decoded RGBA8 pixels.
unsafe fn create_srv(
    device: &ID3D11Device,
    w: u32,
    h: u32,
    pixels: &[u8],
) -> Result<ID3D11ShaderResourceView, Box<dyn std::error::Error>> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: w,
        Height: h,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        ..Default::default()
    };
    let init_data = D3D11_SUBRESOURCE_DATA {
        pSysMem: pixels.as_ptr() as *const c_void,
        SysMemPitch: w * 4,
        SysMemSlicePitch: 0,
    };
    let mut tex: Option<ID3D11Texture2D> = None;
    device.CreateTexture2D(&desc, Some(&init_data), Some(&mut tex))?;
    let tex = tex.ok_or("CreateTexture2D returned null")?;
    let mut srv: Option<ID3D11ShaderResourceView> = None;
    device.CreateShaderResourceView(&tex, None, Some(&mut srv))?;
    Ok(srv.ok_or("CreateShaderResourceView returned null")?)
}
