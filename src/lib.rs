extern crate image;
#[cfg(feature = "jpeg")] extern crate exif;
#[cfg(feature = "jpeg")] extern crate mozjpeg_sys;
#[cfg(feature = "png")] extern crate oxipng;
#[cfg(feature = "webp")] extern crate libwebp_sys;

#[cfg(feature = "jpeg")] pub mod jpeg;
