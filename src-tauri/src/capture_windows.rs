use crate::{lighting, protocol::Rgb, settings::Settings};
use std::ffi::c_void;
use windows::Win32::Graphics::Gdi::*;
use windows::{
    core::Interface,
    Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_UNKNOWN,
            Direct3D11::*,
            Dxgi::{Common::*, *},
        },
    },
};

pub struct Desktop {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    staging: Option<ID3D11Texture2D>,
    width: u32,
    height: u32,
}
struct Acquired<'a>(&'a IDXGIOutputDuplication);
impl Drop for Acquired<'_> {
    fn drop(&mut self) {
        unsafe {
            let _ = self.0.ReleaseFrame();
        }
    }
}
impl Desktop {
    pub fn new(name: &str) -> Result<Self, String> {
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1().map_err(|e| e.to_string())?;
            let mut adapter_index = 0;
            while let Ok(adapter) = factory.EnumAdapters1(adapter_index) {
                adapter_index += 1;
                let mut output_index = 0;
                while let Ok(output) = adapter.EnumOutputs(output_index) {
                    output_index += 1;
                    let description = output.GetDesc().map_err(|e| e.to_string())?;
                    let n = description
                        .DeviceName
                        .iter()
                        .position(|c| *c == 0)
                        .unwrap_or(description.DeviceName.len());
                    if String::from_utf16_lossy(&description.DeviceName[..n]) != name {
                        continue;
                    }
                    if description.Rotation != DXGI_MODE_ROTATION_IDENTITY {
                        return Err("Rotated display uses GDI capture".into());
                    }
                    let (mut device, mut context) = (None, None);
                    D3D11CreateDevice(
                        &adapter,
                        D3D_DRIVER_TYPE_UNKNOWN,
                        HMODULE::default(),
                        D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                        None,
                        D3D11_SDK_VERSION,
                        Some(&mut device),
                        None,
                        Some(&mut context),
                    )
                    .map_err(|e| e.to_string())?;
                    let device = device.ok_or("Missing D3D device")?;
                    let context = context.ok_or("Missing D3D context")?;
                    let output: IDXGIOutput1 = output.cast().map_err(|e| e.to_string())?;
                    let duplication = output.DuplicateOutput(&device).map_err(|e| e.to_string())?;
                    return Ok(Self {
                        device,
                        context,
                        duplication,
                        staging: None,
                        width: 0,
                        height: 0,
                    });
                }
            }
            Err("Display not available through Desktop Duplication".into())
        }
    }
    pub fn colors(&mut self, settings: &Settings) -> Result<Vec<Rgb>, String> {
        unsafe {
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource: Option<IDXGIResource> = None;
            match self
                .duplication
                .AcquireNextFrame(0, &mut info, &mut resource)
            {
                Ok(()) => {
                    let _frame = Acquired(&self.duplication);
                    let source: ID3D11Texture2D = resource
                        .ok_or("Empty desktop frame")?
                        .cast()
                        .map_err(|e| e.to_string())?;
                    let mut desc = D3D11_TEXTURE2D_DESC::default();
                    source.GetDesc(&mut desc);
                    if desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM {
                        return Err("Unsupported desktop pixel format".into());
                    }
                    if self.staging.is_none()
                        || (self.width, self.height) != (desc.Width, desc.Height)
                    {
                        self.width = desc.Width;
                        self.height = desc.Height;
                        desc.Usage = D3D11_USAGE_STAGING;
                        desc.BindFlags = 0;
                        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                        desc.MiscFlags = 0;
                        self.device
                            .CreateTexture2D(&desc, None, Some(&mut self.staging))
                            .map_err(|e| e.to_string())?;
                    }
                    self.context.CopyResource(
                        self.staging.as_ref().ok_or("Missing staging surface")?,
                        &source,
                    );
                }
                Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => {}
                Err(e) => {
                    return Err(format!(
                        "Desktop capture ended: {e}. Start capture to retry."
                    ))
                }
            }
            let Some(staging) = self.staging.as_ref() else {
                return Ok(vec![[0; 3]; settings.count]);
            };
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .map_err(|e| e.to_string())?;
            // Map holds CPU ownership until Unmap. Sample directly using RowPitch;
            // only edge samples are read, with no full-frame conversion/allocation.
            let pixels = std::slice::from_raw_parts(
                mapped.pData.cast::<u8>(),
                mapped.RowPitch as usize * self.height as usize,
            );
            let colors = lighting::sample_strided(
                pixels,
                self.width,
                self.height,
                mapped.RowPitch as usize,
                true,
                settings,
            );
            self.context.Unmap(staging, 0);
            Ok(colors)
        }
    }
}

// Reuse a small top-down DIB. GDI downsamples before CPU readback, avoiding a
// fresh 4K allocation and full-frame color conversion for each lighting frame.
pub struct Thumbnail {
    source: HDC,
    memory: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut c_void,
    pub width: u32,
    pub height: u32,
    rgba: Vec<u8>,
}
impl Thumbnail {
    pub fn new(width: u32, height: u32) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err("Display has no pixels".into());
        }
        let small_width = width.min(320);
        let small_height =
            ((height as f64 / width as f64 * small_width as f64).round() as u32).clamp(1, 2048);
        let mut result = Self {
            source: HDC::default(),
            memory: HDC::default(),
            bitmap: HBITMAP::default(),
            old: HGDIOBJ::default(),
            bits: std::ptr::null_mut(),
            width: small_width,
            height: small_height,
            rgba: vec![0; (small_width * small_height * 4) as usize],
        };
        unsafe {
            result.source = GetDC(None);
            if result.source.is_invalid() {
                return Err("Could not open display context".into());
            }
            result.memory = CreateCompatibleDC(Some(result.source));
            if result.memory.is_invalid() {
                return Err("Could not create capture context".into());
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: small_width as i32,
                    biHeight: -(small_height as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    ..Default::default()
                },
                ..Default::default()
            };
            result.bitmap = CreateDIBSection(
                Some(result.source),
                &info,
                DIB_RGB_COLORS,
                &mut result.bits,
                None,
                0,
            )
            .map_err(|e| e.to_string())?;
            result.old = SelectObject(result.memory, result.bitmap.into());
            if result.old.is_invalid() || result.bits.is_null() {
                return Err("Could not select capture bitmap".into());
            }
            SetStretchBltMode(result.memory, HALFTONE);
        }
        Ok(result)
    }
    pub fn grab(&mut self, x: i32, y: i32, width: u32, height: u32) -> Result<&[u8], String> {
        unsafe {
            if !StretchBlt(
                self.memory,
                0,
                0,
                self.width as i32,
                self.height as i32,
                Some(self.source),
                x,
                y,
                width as i32,
                height as i32,
                ROP_CODE(SRCCOPY.0 | CAPTUREBLT.0),
            )
            .as_bool()
            {
                return Err("Display capture failed; refresh displays and restart capture".into());
            }
            if !GdiFlush().as_bool() {
                return Err("Display capture could not finish".into());
            }
            // GDI has completed writing the selected DIB. The buffer belongs to
            // this object and only this worker thread reads or mutates it.
            let pixels = std::slice::from_raw_parts(self.bits.cast::<u8>(), self.rgba.len());
            for (output, input) in self
                .rgba
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(pixels.as_chunks::<4>().0.iter())
            {
                output.copy_from_slice(&[input[2], input[1], input[0], 255]);
            }
        }
        Ok(&self.rgba)
    }
}
impl Drop for Thumbnail {
    fn drop(&mut self) {
        unsafe {
            if !self.old.is_invalid() {
                SelectObject(self.memory, self.old);
            }
            if !self.bitmap.is_invalid() {
                let _ = DeleteObject(self.bitmap.into());
            }
            if !self.memory.is_invalid() {
                let _ = DeleteDC(self.memory);
            }
            if !self.source.is_invalid() {
                ReleaseDC(None, self.source);
            }
        }
    }
}
