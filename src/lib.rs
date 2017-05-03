extern crate libc;
extern crate image;
#[cfg(feature = "jpeg")] extern crate exif;
#[cfg(feature = "jpeg")] extern crate mozjpeg_sys;
#[cfg(feature = "png")] extern crate oxipng;
#[cfg(feature = "webp")] extern crate libwebp_sys;

pub mod meta;
pub mod srcs;
#[cfg(feature = "jpeg")] pub mod jpeg;
