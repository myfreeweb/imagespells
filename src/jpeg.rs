use std::{mem, ptr, cell, slice};
use std::io::{BufRead};
use image::*;
use mozjpeg_sys::*;
use meta::*;

struct ClientData<R> {
    reader: R,
}

pub struct MozJPEGDecoder<R> {
    cdata: cell::UnsafeCell<ClientData<R>>,
    src: cell::UnsafeCell<jpeg_source_mgr>,
    err: jpeg_error_mgr,
    cinfo: jpeg_decompress_struct,
    header_read: bool,
}

extern "C" fn init_source<R>(_: &mut jpeg_decompress_struct) { }

#[allow(mutable_transmutes)]
extern "C" fn fill_input_buffer<R: BufRead>(cinfo: &mut jpeg_decompress_struct) -> boolean {
    let cdata = unsafe { &mut *(*(cinfo.common.client_data as *mut cell::UnsafeCell<ClientData<R>>)).get() };
    match cdata.reader.fill_buf() {
        Ok(buf) => {
            unsafe {
                // lol it's just not 'const' in the C code and i have to do *this*
                (*cinfo.src).next_input_byte = mem::transmute::<&[u8], &mut [u8]>(buf).as_mut_ptr();
                (*cinfo.src).bytes_in_buffer = buf.len();
            }
            true as boolean
        },
        Err(_) => false as boolean
    }
}

extern "C" fn skip_input_data<R: BufRead>(cinfo: &mut jpeg_decompress_struct, num_bytes: c_long) {
    let cdata = unsafe { &mut *(*(cinfo.common.client_data as *mut cell::UnsafeCell<ClientData<R>>)).get() };
    cdata.reader.consume(num_bytes as usize);
    unsafe {
        (*cinfo.src).next_input_byte = (*cinfo.src).next_input_byte.offset(num_bytes as isize);
        (*cinfo.src).bytes_in_buffer -= num_bytes as usize;
    }
}

extern "C" fn resync_to_restart(cinfo: &mut jpeg_decompress_struct, desired: c_int) -> boolean {
    unsafe { jpeg_resync_to_restart(cinfo, desired) }
}

extern "C" fn term_source<R>(_: &mut jpeg_decompress_struct) { }

impl<R: BufRead> MozJPEGDecoder<R> {
    pub fn new(r: R) -> MozJPEGDecoder<R> {
        let mut dec = MozJPEGDecoder {
            cdata: cell::UnsafeCell::new(ClientData {
                reader: r,
            }),
            src: cell::UnsafeCell::new(jpeg_source_mgr {
                next_input_byte: ptr::null(),
                bytes_in_buffer: 0,
                init_source: Some(init_source::<R>),
                fill_input_buffer: Some(fill_input_buffer::<R>),
                skip_input_data: Some(skip_input_data::<R>),
                resync_to_restart: Some(resync_to_restart),
                term_source: Some(term_source::<R>),
            }),
            err: unsafe { mem::zeroed() },
            cinfo: unsafe { mem::zeroed() },
            header_read: false,
        };
        let size: size_t = mem::size_of_val(&dec.cinfo);
        dec.cinfo.common.err = unsafe { jpeg_std_error(&mut dec.err) };
        unsafe { jpeg_CreateDecompress(&mut dec.cinfo, JPEG_LIB_VERSION, size); }
        dec.cinfo.common.client_data = &mut dec.cdata as *mut _ as *mut c_void;
        dec.cinfo.src = dec.src.get();
        dec
    }

    fn ensure_header(&mut self) {
        if !self.header_read {
            unsafe {
                jpeg_save_markers(&mut self.cinfo, jpeg_marker::COM as i32, 0xffff);       // Comments
                jpeg_save_markers(&mut self.cinfo, jpeg_marker::APP0 as i32 + 1, 0xffff);  // Exif/XMP
                jpeg_save_markers(&mut self.cinfo, jpeg_marker::APP0 as i32 + 2, 0xffff);  // ICC
                jpeg_save_markers(&mut self.cinfo, jpeg_marker::APP0 as i32 + 13, 0xffff); // IPTC
                jpeg_read_header(&mut self.cinfo, true as boolean);
                jpeg_start_decompress(&mut self.cinfo);
            }
            // The image crate doesn't care about YUV/CMYK/whatever, so just let libjpeg convert to RGB
            self.cinfo.out_color_space = match self.cinfo.jpeg_color_space {
                JCS_GRAYSCALE => JCS_GRAYSCALE,
                _ => JCS_RGB,
            };
            self.header_read = true;
        }
    }
}

impl<R> Drop for MozJPEGDecoder<R> {
    #[inline]
    fn drop(&mut self) {
        unsafe { jpeg_destroy_decompress(&mut self.cinfo) }
    }
}

impl<R: BufRead> MetadataDecoder for MozJPEGDecoder<R> {
    fn raw_metadata(&mut self) -> Vec<(MetadataType, &[u8])> {
        self.ensure_header();
        let mut marker : *mut jpeg_marker_struct = self.cinfo.marker_list;
        let mut result = Vec::new();
        while marker != ptr::null_mut() {
            unsafe {
                let t = if (*marker).marker == jpeg_marker::COM as u8 {
                    MetadataType::Comment
                } else if (*marker).marker == jpeg_marker::APP0 as u8 + 1 {
                    MetadataType::ExifXmp
                } else if (*marker).marker == jpeg_marker::APP0 as u8 + 2 {
                    MetadataType::Icc
                } else if (*marker).marker == jpeg_marker::APP0 as u8 + 13 {
                    MetadataType::Iptc
                } else {
                    MetadataType::Unknown
                };
                result.push((t, slice::from_raw_parts((*marker).data, (*marker).data_length as usize)));
                marker = (*marker).next;
            }
        }
        result
    }
}

impl<R: BufRead> ImageDecoder for MozJPEGDecoder<R> {
    fn dimensions(&mut self) -> ImageResult<(u32, u32)> {
        self.ensure_header();
        Ok((self.cinfo.output_width as u32, self.cinfo.output_height as u32))
    }

    fn colortype(&mut self) -> ImageResult<ColorType> {
        self.ensure_header();
        Ok(match self.cinfo.jpeg_color_space {
            JCS_GRAYSCALE => Gray(8),
            _ => RGB(8),
        })
    }

    fn row_len(&mut self) -> ImageResult<usize> {
        self.ensure_header();
        Ok(self.cinfo.output_width as usize * self.cinfo.output_components as usize)
    }

    fn read_scanline(&mut self, buf: &mut [u8]) -> ImageResult<u32> {
        self.ensure_header();
        if unsafe { jpeg_read_scanlines(&mut self.cinfo, &mut buf.as_mut_ptr(), 1) != 1 } {
            Err(ImageError::ImageEnd)
        } else {
            Ok(self.cinfo.output_scanline)
        }
    }

    fn read_image(&mut self) -> ImageResult<DecodingResult> {
        self.ensure_header();
        let stride = self.cinfo.output_width as isize * self.cinfo.output_components as isize;
        let mut buf = Vec::with_capacity(stride as usize * self.cinfo.output_height as usize);
        let mut bufp = &mut buf.as_mut_ptr() as *mut *mut u8;
        while self.cinfo.output_scanline < self.cinfo.output_height {
            unsafe {
                if jpeg_read_scanlines(&mut self.cinfo, bufp, 1) != 1 {
                    return Err(ImageError::ImageEnd);
                }
                *bufp = buf.as_mut_ptr().offset(self.cinfo.output_scanline as isize * stride);
                let old_len = buf.len();
                buf.set_len(old_len + stride as usize);
            };
        }
        //unsafe { jpeg_finish_decompress(&mut self.cinfo); }
        // XXX: jpeg_finish_decompress prevents reading image after reading metadata?!
        // since desturctor calls jpeg_destroy_decompress anyway, not calling finish is ok?
        Ok(DecodingResult::U8(buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::*;
    use std::io::BufReader;

    #[test]
    fn test_decode() {
        let mut dec = MozJPEGDecoder::new(BufReader::new(File::open("fixtures/example.jpg").unwrap()));
        assert_eq!(dec.dimensions().unwrap(), (4, 4));
        assert_eq!(dec.colortype().unwrap(), ColorType::RGB(8));
        assert_eq!(dec.row_len().unwrap(), 4 * 3);
        assert_eq!(dec.raw_metadata().first().unwrap(), &(MetadataType::Comment, &b"Created with GIMP"[..]));
        if let DecodingResult::U8(img) = dec.read_image().unwrap() {
            assert_eq!(img[0..3], [232, 193, 238]);
        } else {
            panic!("wtf");
        }
    }
}
