//! Native PNG encoder, loaded from System32 only when an export requests it.
use crate::{
    drawing::{self, Canvas, Result},
    ink_io::PageSnapshot,
};
use std::{
    ffi::{CStr, c_void},
    ptr::null_mut,
};
use windows::{
    Foundation::Point,
    Win32::{
        Foundation::{FreeLibrary, HGLOBAL, HMODULE},
        Graphics::GdiPlus::{
            CombineMode, CombineModeReplace, EncoderParameters, FillMode, FillModeWinding,
            GdiplusStartupInput, GdiplusStartupOutput, GpBitmap, GpBrush, GpGraphics, GpImage,
            GpPath, GpPen, GpSolidFill, LineCap, LineCapRound, LineJoin, LineJoinRound,
            MatrixOrder, MatrixOrderPrepend, Ok as GdiOk, PixelOffsetMode, PixelOffsetModeHalf,
            SmoothingMode, SmoothingModeAntiAlias, Status, Unit, UnitPixel, UnitWorld,
        },
        System::{
            Com::{
                STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET, StructuredStorage::CreateStreamOnHGlobal,
            },
            LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW},
        },
    },
    core::{GUID, Interface, PCSTR, w},
};

pub const DPI: f32 = 300.0;

struct Module(HMODULE);
impl Drop for Module {
    fn drop(&mut self) {
        unsafe {
            let _ = FreeLibrary(self.0);
        }
    }
}

unsafe fn symbol(module: HMODULE, name: &CStr) -> Result<unsafe extern "system" fn() -> isize> {
    let function = unsafe { GetProcAddress(module, PCSTR(name.as_ptr().cast())) }
        .ok_or_else(|| std::io::Error::other("PNG用のWindows APIが利用できません。"))?;
    Ok(function)
}
fn check(status: Status) -> Result<()> {
    if status == GdiOk {
        std::result::Result::Ok(())
    } else {
        Err(std::io::Error::other(format!("PNGの画像処理に失敗しました ({})。", status.0)).into())
    }
}

macro_rules! native_api {
    ($($field:ident: $name:expr => $ty:ty),+ $(,)?)=> {
        struct Native { $($field:$ty,)+ token:usize, _module:Module }
        impl Native {
            fn new()->Result<Self> {
                let module=Module(unsafe {LoadLibraryExW(w!("gdiplus.dll"),None,LOAD_LIBRARY_SEARCH_SYSTEM32)?});
                // Every signature is a fixed GDI+ SDK declaration below; the
                // module is System32-owned and outlives all function pointers.
                let mut api=Self {$($field:unsafe {std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(symbol(module.0,$name)?)},)+ token:0,_module:module};
                let input=GdiplusStartupInput {GdiplusVersion:1,SuppressExternalCodecs:true.into(),..Default::default()};
                let mut token=0;
                check(unsafe {(api.startup)(&mut token,&input,null_mut())})?;
                api.token=token;
                std::result::Result::Ok(api)
            }
        }
        impl Drop for Native { fn drop(&mut self) {if self.token!=0 {unsafe {(self.shutdown)(self.token)}}} }
    }
}
native_api! {
    startup: c"GdiplusStartup" => unsafe extern "system" fn(*mut usize,*const GdiplusStartupInput,*mut GdiplusStartupOutput)->Status,
    shutdown: c"GdiplusShutdown" => unsafe extern "system" fn(usize),
    bitmap: c"GdipCreateBitmapFromScan0" => unsafe extern "system" fn(i32,i32,i32,i32,*const u8,*mut *mut GpBitmap)->Status,
    resolution: c"GdipBitmapSetResolution" => unsafe extern "system" fn(*mut GpBitmap,f32,f32)->Status,
    graphics: c"GdipGetImageGraphicsContext" => unsafe extern "system" fn(*mut GpImage,*mut *mut GpGraphics)->Status,
    clear: c"GdipGraphicsClear" => unsafe extern "system" fn(*mut GpGraphics,u32)->Status,
    unit: c"GdipSetPageUnit" => unsafe extern "system" fn(*mut GpGraphics,Unit)->Status,
    smoothing: c"GdipSetSmoothingMode" => unsafe extern "system" fn(*mut GpGraphics,SmoothingMode)->Status,
    pixel_offset: c"GdipSetPixelOffsetMode" => unsafe extern "system" fn(*mut GpGraphics,PixelOffsetMode)->Status,
    scale: c"GdipScaleWorldTransform" => unsafe extern "system" fn(*mut GpGraphics,f32,f32,MatrixOrder)->Status,
    clip: c"GdipSetClipRect" => unsafe extern "system" fn(*mut GpGraphics,f32,f32,f32,f32,CombineMode)->Status,
    pen: c"GdipCreatePen1" => unsafe extern "system" fn(u32,f32,Unit,*mut *mut GpPen)->Status,
    pen_color: c"GdipSetPenColor" => unsafe extern "system" fn(*mut GpPen,u32)->Status,
    pen_width: c"GdipSetPenWidth" => unsafe extern "system" fn(*mut GpPen,f32)->Status,
    start_cap: c"GdipSetPenStartCap" => unsafe extern "system" fn(*mut GpPen,LineCap)->Status,
    end_cap: c"GdipSetPenEndCap" => unsafe extern "system" fn(*mut GpPen,LineCap)->Status,
    join: c"GdipSetPenLineJoin" => unsafe extern "system" fn(*mut GpPen,LineJoin)->Status,
    path: c"GdipCreatePath" => unsafe extern "system" fn(FillMode,*mut *mut GpPath)->Status,
    reset: c"GdipResetPath" => unsafe extern "system" fn(*mut GpPath)->Status,
    line: c"GdipAddPathLine" => unsafe extern "system" fn(*mut GpPath,f32,f32,f32,f32)->Status,
    curve: c"GdipAddPathBezier" => unsafe extern "system" fn(*mut GpPath,f32,f32,f32,f32,f32,f32,f32,f32)->Status,
    draw: c"GdipDrawPath" => unsafe extern "system" fn(*mut GpGraphics,*mut GpPen,*mut GpPath)->Status,
    brush: c"GdipCreateSolidFill" => unsafe extern "system" fn(u32,*mut *mut GpSolidFill)->Status,
    ellipse: c"GdipFillEllipse" => unsafe extern "system" fn(*mut GpGraphics,*mut GpBrush,f32,f32,f32,f32)->Status,
    save: c"GdipSaveImageToStream" => unsafe extern "system" fn(*mut GpImage,*mut c_void,*const GUID,*const EncoderParameters)->Status,
    delete_graphics: c"GdipDeleteGraphics" => unsafe extern "system" fn(*mut GpGraphics)->Status,
    delete_pen: c"GdipDeletePen" => unsafe extern "system" fn(*mut GpPen)->Status,
    delete_path: c"GdipDeletePath" => unsafe extern "system" fn(*mut GpPath)->Status,
    delete_brush: c"GdipDeleteBrush" => unsafe extern "system" fn(*mut GpBrush)->Status,
    delete_image: c"GdipDisposeImage" => unsafe extern "system" fn(*mut GpImage)->Status,
}

pub struct Encoder(Native);
impl Encoder {
    pub fn new() -> Result<Self> {
        std::result::Result::Ok(Self(Native::new()?))
    }
    pub fn encode(&self, page: &PageSnapshot, budget: &mut drawing::DrawBudget) -> Result<Vec<u8>> {
        let (mm_w, mm_h) = page.page.paper.millimeters();
        let width = (mm_w * DPI / 25.4).round() as i32;
        let height = (mm_h * DPI / 25.4).round() as i32;
        let mut canvas = Raster {
            api: &self.0,
            image: null_mut(),
            graphics: null_mut(),
            pen: null_mut(),
            path: null_mut(),
            point: Point::default(),
        };
        unsafe {
            // PixelFormat32bppARGB: canonical ARGB, allocated by GDI+.
            check((self.0.bitmap)(
                width,
                height,
                0,
                0x0026200a,
                std::ptr::null(),
                &mut canvas.image,
            ))?;
            check((self.0.resolution)(canvas.image, DPI, DPI))?;
            check((self.0.graphics)(canvas.image.cast(), &mut canvas.graphics))?;
            check((self.0.clear)(canvas.graphics, 0xffffffff))?;
            check((self.0.unit)(canvas.graphics, UnitPixel))?;
            check((self.0.smoothing)(canvas.graphics, SmoothingModeAntiAlias))?;
            check((self.0.pixel_offset)(canvas.graphics, PixelOffsetModeHalf))?;
            check((self.0.scale)(
                canvas.graphics,
                DPI / 96.0,
                DPI / 96.0,
                MatrixOrderPrepend,
            ))?;
            let (w, h) = page.page.paper.dips();
            check((self.0.clip)(
                canvas.graphics,
                0.0,
                0.0,
                w,
                h,
                CombineModeReplace,
            ))?;
            // Pen widths are DIPs and must follow the 300/96 world transform.
            check((self.0.pen)(0xff000000, 1.0, UnitWorld, &mut canvas.pen))?;
            check((self.0.start_cap)(canvas.pen, LineCapRound))?;
            check((self.0.end_cap)(canvas.pen, LineCapRound))?;
            check((self.0.join)(canvas.pen, LineJoinRound))?;
            check((self.0.path)(FillModeWinding, &mut canvas.path))?;
        }
        drawing::draw_page(&mut canvas, page, budget)?;
        unsafe {
            check((self.0.delete_graphics)(canvas.graphics))?;
            canvas.graphics = null_mut();
        }
        let stream = unsafe { CreateStreamOnHGlobal(HGLOBAL::default(), true)? };
        let encoder = GUID::from_u128(0x557cf406_1a04_11d3_9a73_0000f81ef32e);
        unsafe {
            check((self.0.save)(
                canvas.image.cast(),
                stream.as_raw(),
                &encoder,
                std::ptr::null(),
            ))?;
        }
        let mut stat = STATSTG::default();
        unsafe {
            stream.Stat(&mut stat, STATFLAG_NONAME)?;
            stream.Seek(0, STREAM_SEEK_SET, None)?;
        }
        if stat.cbSize > 128 * 1024 * 1024 {
            return Err("PNGの出力サイズが大きすぎます。".into());
        }
        let mut bytes = vec![0; stat.cbSize as usize];
        let mut read = 0;
        unsafe {
            stream
                .Read(
                    bytes.as_mut_ptr().cast(),
                    bytes.len() as u32,
                    Some(&mut read),
                )
                .ok()?;
        }
        if read as usize != bytes.len() {
            return Err("PNGデータを読み取れませんでした。".into());
        }
        std::result::Result::Ok(bytes)
    }
}

struct Raster<'a> {
    api: &'a Native,
    image: *mut GpBitmap,
    graphics: *mut GpGraphics,
    pen: *mut GpPen,
    path: *mut GpPath,
    point: Point,
}
impl Canvas for Raster<'_> {
    fn begin(&mut self, color: u32, width: f32, p: Point) -> Result<()> {
        unsafe {
            check((self.api.reset)(self.path))?;
            check((self.api.pen_color)(self.pen, color))?;
            check((self.api.pen_width)(self.pen, width))?;
        }
        self.point = p;
        std::result::Result::Ok(())
    }
    fn line(&mut self, p: Point) -> Result<()> {
        unsafe {
            check((self.api.line)(
                self.path,
                self.point.X,
                self.point.Y,
                p.X,
                p.Y,
            ))?;
        }
        self.point = p;
        std::result::Result::Ok(())
    }
    fn curve(&mut self, a: Point, b: Point, p: Point) -> Result<()> {
        unsafe {
            check((self.api.curve)(
                self.path,
                self.point.X,
                self.point.Y,
                a.X,
                a.Y,
                b.X,
                b.Y,
                p.X,
                p.Y,
            ))?;
        }
        self.point = p;
        std::result::Result::Ok(())
    }
    fn end(&mut self) -> Result<()> {
        unsafe { check((self.api.draw)(self.graphics, self.pen, self.path)) }
    }
    fn dot(&mut self, color: u32, p: Point, r: f32) -> Result<()> {
        let mut brush = null_mut();
        unsafe {
            check((self.api.brush)(color, &mut brush))?;
            let result = (self.api.ellipse)(
                self.graphics,
                brush.cast(),
                p.X - r,
                p.Y - r,
                r * 2.0,
                r * 2.0,
            );
            (self.api.delete_brush)(brush.cast());
            check(result)
        }
    }
}
impl Drop for Raster<'_> {
    fn drop(&mut self) {
        unsafe {
            if !self.graphics.is_null() {
                (self.api.delete_graphics)(self.graphics);
            }
            if !self.path.is_null() {
                (self.api.delete_path)(self.path);
            }
            if !self.pen.is_null() {
                (self.api.delete_pen)(self.pen);
            }
            if !self.image.is_null() {
                (self.api.delete_image)(self.image.cast());
            }
        }
    }
}
