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
    crop: lighting::Crop,
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
                        crop: lighting::Crop::default(),
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
            let colors = lighting::sample_tracked(
                pixels,
                self.width,
                self.height,
                mapped.RowPitch as usize,
                true,
                settings,
                &mut self.crop,
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

// A retained Windows Graphics Capture session targets only the selected window.
// Poll its free-threaded frame pool; never call another application's PrintWindow.
use windows::Graphics::{
    Capture::{
        Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem,
        GraphicsCaptureSession,
    },
    DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
    SizeInt32,
};
use windows::Win32::System::WinRT::{
    Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
    Graphics::Capture::IGraphicsCaptureItemInterop,
    RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED,
};
struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { RoUninitialize() };
    }
}
struct WindowFrame(Direct3D11CaptureFrame);
impl Drop for WindowFrame {
    fn drop(&mut self) {
        let _ = self.0.Close();
    }
}
pub struct WindowCapture {
    window: usize,
    pid: u32,
    session: GraphicsCaptureSession,
    pool: Direct3D11CaptureFramePool,
    item: GraphicsCaptureItem,
    closed_token: i64,
    closed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    runtime_device: IDirect3DDevice,
    staging: Option<ID3D11Texture2D>,
    size: SizeInt32,
    pixels: (u32, u32),
    crop: lighting::Crop,
    started: std::time::Instant,
    _apartment: Apartment,
}
impl WindowCapture {
    pub fn new(id: u32) -> Result<Self, String> {
        Self::create(id).map_err(|e| {
            format!("Window capture unavailable: {e}. Windows 10 1903 or newer is required.")
        })
    }
    fn create(id: u32) -> windows::core::Result<Self> {
        unsafe {
            RoInitialize(RO_INIT_MULTITHREADED)?;
        }
        let apartment = Apartment;
        let (mut device, mut context) = (None, None);
        unsafe {
            D3D11CreateDevice(
                None,
                windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )?;
        }
        let device = device.ok_or_else(windows::core::Error::empty)?;
        let context = context.ok_or_else(windows::core::Error::empty)?;
        let dxgi: IDXGIDevice = device.cast()?;
        let runtime_device: IDirect3DDevice =
            unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi)? }.cast()?;
        let interop: IGraphicsCaptureItemInterop =
            windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        let item: GraphicsCaptureItem = unsafe {
            interop.CreateForWindow(windows::Win32::Foundation::HWND(id as usize as *mut c_void))?
        };
        let size = item.Size()?;
        if size.Width <= 0
            || size.Height <= 0
            || i64::from(size.Width) * i64::from(size.Height) > 40_000_000
        {
            return Err(windows::core::Error::from_hresult(
                windows::Win32::Foundation::E_INVALIDARG,
            ));
        }
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &runtime_device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2,
            size,
        )?;
        let session = pool.CreateCaptureSession(&item)?;
        let _ = session.SetIsCursorCaptureEnabled(false);
        // Keep the OS capture indicator; do not request borderless capture.
        let closed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = closed.clone();
        let closed_token =
            item.Closed(&windows::Foundation::TypedEventHandler::new(move |_, _| {
                flag.store(true, std::sync::atomic::Ordering::Release);
                Ok(())
            }))?;
        let mut pid = 0;
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
                windows::Win32::Foundation::HWND(id as usize as *mut c_void),
                Some(&mut pid),
            );
        }
        let capture = Self {
            window: id as usize,
            pid,
            session,
            pool,
            item,
            closed_token,
            closed,
            device,
            context,
            runtime_device,
            staging: None,
            size,
            pixels: (0, 0),
            crop: lighting::Crop::default(),
            started: std::time::Instant::now(),
            _apartment: apartment,
        };
        capture.session.StartCapture()?;
        Ok(capture)
    }
    pub fn colors(&mut self, settings: &Settings, aspect: f32) -> Result<Vec<Rgb>, String> {
        self.read(settings, aspect)
            .map_err(|e| format!("Window capture ended: {e}. Select the source and Retry."))
    }
    fn read(&mut self, settings: &Settings, aspect: f32) -> windows::core::Result<Vec<Rgb>> {
        let hwnd = windows::Win32::Foundation::HWND(self.window as *mut c_void);
        let mut pid = 0;
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, Some(&mut pid));
        }
        if self.closed.load(std::sync::atomic::Ordering::Acquire)
            || pid != self.pid
            || pid == 0
            || unsafe {
                !windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(hwnd)).as_bool()
                    || windows::Win32::UI::WindowsAndMessaging::IsIconic(hwnd).as_bool()
            }
        {
            return Err(windows::core::Error::from_hresult(
                windows::Win32::Foundation::E_ABORT,
            ));
        }
        match self.pool.TryGetNextFrame() {
            Ok(frame) => {
                let frame = WindowFrame(frame);
                let content = frame.0.ContentSize()?;
                if content.Width <= 0
                    || content.Height <= 0
                    || i64::from(content.Width) * i64::from(content.Height) > 40_000_000
                {
                    return Err(windows::core::Error::from_hresult(
                        windows::Win32::Foundation::E_INVALIDARG,
                    ));
                }
                let access: IDirect3DDxgiInterfaceAccess = frame.0.Surface()?.cast()?;
                let texture: ID3D11Texture2D = unsafe { access.GetInterface()? };
                let mut desc = D3D11_TEXTURE2D_DESC::default();
                unsafe {
                    texture.GetDesc(&mut desc);
                }
                if desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM {
                    return Err(windows::core::Error::from_hresult(
                        windows::Win32::Foundation::E_INVALIDARG,
                    ));
                }
                if self.staging.is_none() || self.pixels != (desc.Width, desc.Height) {
                    self.pixels = (desc.Width, desc.Height);
                    desc.Usage = D3D11_USAGE_STAGING;
                    desc.BindFlags = 0;
                    desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                    desc.MiscFlags = 0;
                    unsafe {
                        self.device
                            .CreateTexture2D(&desc, None, Some(&mut self.staging))?;
                    }
                }
                unsafe {
                    self.context
                        .CopyResource(self.staging.as_ref().unwrap(), &texture);
                }
                drop(frame);
                if self.size != content {
                    self.pool.Recreate(
                        &self.runtime_device,
                        DirectXPixelFormat::B8G8R8A8UIntNormalized,
                        2,
                        content,
                    )?;
                    self.size = content;
                    self.started = std::time::Instant::now();
                    self.staging = None; // Avoid displaying stale padding while the next-sized frame arrives.
                }
            }
            // windows-rs represents a successful null frame as Error(S_OK).
            Err(e) if e.code().is_ok() || e.code() == windows::Win32::Foundation::E_POINTER => {}
            Err(e) => return Err(e),
        }
        let Some(staging) = self.staging.as_ref() else {
            if self.started.elapsed() > std::time::Duration::from_secs(5) {
                return Err(windows::core::Error::from_hresult(
                    windows::Win32::Foundation::E_FAIL,
                ));
            }
            return Ok(vec![[0; 3]; settings.count]);
        };
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            self.context
                .Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let pixels = std::slice::from_raw_parts(
                mapped.pData.cast::<u8>(),
                mapped.RowPitch as usize * self.pixels.1 as usize,
            );
            let colors = lighting::sample_geometry(
                pixels,
                (self.pixels.0, self.pixels.1, mapped.RowPitch as usize, true),
                settings,
                &mut self.crop,
                aspect,
            );
            self.context.Unmap(staging, 0);
            Ok(colors)
        }
    }
}
impl Drop for WindowCapture {
    fn drop(&mut self) {
        let _ = self.item.RemoveClosed(self.closed_token);
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

#[cfg(test)]
mod window_tests {
    use super::*;
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc, Arc,
        },
        time::Duration,
    };
    use windows::{core::w, Win32::UI::WindowsAndMessaging::*};
    struct Fixture {
        stop: Arc<AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(thread) = self.thread.take() {
                thread.join().unwrap();
            }
        }
    }
    #[test]
    #[ignore = "requires an interactive Windows desktop with Windows Graphics Capture"]
    fn selected_window_capture_resizes_and_closes_without_desktop_fallback() {
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::spawn(move || unsafe {
            let window = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!("DynamicLight capture fixture"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                50,
                50,
                480,
                320,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            tx.send(window.0 as usize).unwrap();
            while !done.load(Ordering::Acquire) {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            DestroyWindow(window).unwrap();
        });
        let fixture = Fixture {
            stop,
            thread: Some(thread),
        };
        let hwnd = windows::Win32::Foundation::HWND(
            rx.recv_timeout(Duration::from_secs(5)).unwrap() as *mut c_void,
        );
        let mut capture = WindowCapture::new(hwnd.0 as usize as u32).unwrap();
        let settings = Settings::default();
        let mut frame = vec![];
        for _ in 0..100 {
            frame = capture.colors(&settings, 16.0 / 9.0).unwrap();
            if frame.iter().flatten().any(|v| *v > 0) {
                break;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        assert_eq!(frame.len(), settings.count);
        assert!(
            frame.iter().flatten().any(|v| *v > 0),
            "Fixture window produced no frame"
        );
        let before = capture.size;
        unsafe {
            SetWindowPos(
                hwnd,
                None,
                0,
                0,
                620,
                400,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
            .unwrap();
        }
        for _ in 0..100 {
            capture.colors(&settings, 16.0 / 9.0).unwrap();
            if capture.size != before {
                break;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        assert_ne!(capture.size, before);
        drop(fixture);
        for _ in 0..100 {
            if capture.colors(&settings, 16.0 / 9.0).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        panic!("Closed window did not terminate capture");
    }
}
