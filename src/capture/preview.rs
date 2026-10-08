//! Small, worker-prepared screenshot assets. No decoding or file I/O in rendering.
use std::path::PathBuf;
use std::sync::Arc;

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::utils::Transform;

#[derive(Clone, Debug)]
pub struct ScreenshotPreview {
    pub path: PathBuf,
    pub buffer: MemoryRenderBuffer,
    pub size: (i32, i32),
    pub png: Arc<[u8]>,
}

pub fn prepare(
    path: PathBuf,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
) -> Option<Arc<ScreenshotPreview>> {
    let image = image::RgbaImage::from_raw(width, height, pixels)?;
    let thumbnail = image::imageops::thumbnail(&image, width.min(656), height.min(328));
    let size = (thumbnail.width() as i32, thumbnail.height() as i32);
    let buffer = MemoryRenderBuffer::from_slice(
        thumbnail.as_raw(),
        Fourcc::Abgr8888,
        size,
        1,
        Transform::Normal,
        None,
    );
    let png = std::fs::read(&path).ok()?.into();
    Some(Arc::new(ScreenshotPreview {
        path,
        buffer,
        size,
        png,
    }))
}
