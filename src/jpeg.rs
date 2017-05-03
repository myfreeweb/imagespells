use std::{mem, ptr, cell, slice, fs, ffi};
use std::os::unix::io::{IntoRawFd};
use std::io::{Write, BufRead};
use libc;
use image::*;
use mozjpeg_sys::*;
use meta::*;
use srcs::*;

struct DecClientData<R> {
    reader: R,
    //last_bytes_in_buffer: usize,
}

pub struct MozJPEGDecoder<R> {
    cdata: cell::UnsafeCell<DecClientData<R>>,
    err: jpeg_error_mgr,
    cinfo: jpeg_decompress_struct,
    cleanup: Option<fn (&mut MozJPEGDecoder<R>)>,
    header_read: bool,
}

/* XXX: buggy :(

extern "C" fn init_source<R>(_: &mut jpeg_decompress_struct) { }

#[allow(mutable_transmutes)]
extern "C" fn fill_input_buffer<R: BufRead>(cinfo: &mut jpeg_decompress_struct) -> boolean {
    let cdata = unsafe { &mut *(*(cinfo.common.client_data as *mut cell::UnsafeCell<DecClientData<R>>)).get() };
    let reader: *mut R = &mut cdata.reader;
    unsafe {
        let diff = cdata.last_bytes_in_buffer - (*cinfo.src).bytes_in_buffer;
        if diff > 0 {
            (*reader).consume(diff);
        }
    }
    match unsafe { (*reader).fill_buf() } {
        Ok(buf) => {
            unsafe {
                // XXX: it's not actually mut
                (*cinfo.src).next_input_byte = mem::transmute::<&[u8], &mut [u8]>(buf).as_mut_ptr();
                (*cinfo.src).bytes_in_buffer = buf.len();
                cdata.last_bytes_in_buffer = buf.len();
            }
            true as boolean
        },
        Err(_) => false as boolean
    }
}

extern "C" fn skip_input_data<R: BufRead>(cinfo: &mut jpeg_decompress_struct, num_bytes: c_long) {
    let cdata = unsafe { &mut *(*(cinfo.common.client_data as *mut cell::UnsafeCell<DecClientData<R>>)).get() };
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


impl<R> MozJPEGDecoder<R> where R: BufRead {
    pub fn for_bufread(r: R) -> MozJPEGDecoder<R> where R: BufRead {
        let mut dec = MozJPEGDecoder::new(r);
        dec.cinfo.src = &mut jpeg_source_mgr {
            next_input_byte: ptr::null(),
            bytes_in_buffer: 0,
            init_source: Some(init_source::<R>),
            fill_input_buffer: Some(fill_input_buffer::<R>),
            skip_input_data: Some(skip_input_data::<R>),
            resync_to_restart: Some(resync_to_restart),
            term_source: Some(term_source::<R>),
        };
        dec
    }
}
*/

fn cleanup_fd(dec: &mut MozJPEGDecoder<*mut FILE>) {
    unsafe { libc::fclose((*dec.cdata.get()).reader) };
}

impl DecoderFromFile for MozJPEGDecoder<*mut FILE> {
    fn for_file(r: fs::File) -> MozJPEGDecoder<*mut FILE> {
        let fd = unsafe { libc::fdopen(r.into_raw_fd(), ffi::CString::new("rb").unwrap().as_ptr()) };
        let mut dec = MozJPEGDecoder::new(fd);
        unsafe { jpeg_stdio_src(&mut dec.cinfo, fd); }
        dec.cleanup = Some(cleanup_fd);
        dec
    }
}

impl<'a> DecoderFromMemory<'a> for MozJPEGDecoder<&'a [u8]> {
    fn for_slice(r: &[u8]) -> MozJPEGDecoder<&[u8]> {
        let mut dec = MozJPEGDecoder::new(r);
        unsafe { jpeg_mem_src(&mut dec.cinfo, r.as_ptr(), r.len() as c_ulong); }
        dec
    }
}

impl<R> MozJPEGDecoder<R> {
    fn new(r: R) -> MozJPEGDecoder<R> {
        let mut dec = MozJPEGDecoder {
            cdata: cell::UnsafeCell::new(DecClientData {
                reader: r,
                //last_bytes_in_buffer: 0,
            }),
            err: unsafe { mem::zeroed() },
            cinfo: unsafe { mem::zeroed() },
            cleanup: None,
            header_read: false,
        };
        let size: size_t = mem::size_of_val(&dec.cinfo);
        dec.cinfo.common.err = unsafe { jpeg_std_error(&mut dec.err) };
        unsafe { jpeg_CreateDecompress(&mut dec.cinfo, JPEG_LIB_VERSION, size); }
        dec.cinfo.common.client_data = &mut dec.cdata as *mut _ as *mut c_void;
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
        if let Some(f) = self.cleanup {
            f(self)
        }
    }
}

impl<R> MetadataDecoder for MozJPEGDecoder<R> {
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

impl<R> ImageDecoder for MozJPEGDecoder<R> {
    fn dimensions(&mut self) -> ImageResult<(u32, u32)> {
        self.ensure_header();
        Ok((self.cinfo.output_width as c_uint, self.cinfo.output_height as c_uint))
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

const OUT_BUF_SIZE: usize = 100;

struct EncClientData<W> {
    writer: W,
    buffer: [u8; OUT_BUF_SIZE],
}

pub struct MozJPEGEncoder<W> {
    cdata: cell::UnsafeCell<EncClientData<W>>,
    err: jpeg_error_mgr,
    cinfo: jpeg_compress_struct,
    cleanup: Option<fn (&mut MozJPEGEncoder<W>)>,
    pub quality: u8,
    pub jpgcrush: bool,
    pub trellis: bool,
    pub trellis_dc: bool,
    pub trellis_eob_opt: bool,
    pub trellis_q_opt: bool,
    pub trellis_scans: bool,
    pub deringing: bool,
}

fn cleanup_fd_enc(enc: &mut MozJPEGEncoder<*mut FILE>) {
    unsafe { libc::fclose((*enc.cdata.get()).writer) };
}

impl MozJPEGEncoder<*mut FILE> {
    pub fn for_file(r: fs::File) -> MozJPEGEncoder<*mut FILE> {
        let fd = unsafe { libc::fdopen(r.into_raw_fd(), ffi::CString::new("wb").unwrap().as_ptr()) };
        let mut enc = MozJPEGEncoder::new(fd);
        unsafe { jpeg_stdio_dest(&mut enc.cinfo, fd); }
        enc.cleanup = Some(cleanup_fd_enc);
        enc
    }
}

/* XXX: WTF
extern "C" fn init_destination<W>(cinfo: &mut jpeg_compress_struct) {
    let cdata = unsafe { &mut *(*(cinfo.common.client_data as *mut cell::UnsafeCell<EncClientData<W>>)).get() };
    unsafe {
        (*cinfo.dest).next_output_byte = cdata.buffer.as_mut_ptr();
        (*cinfo.dest).free_in_buffer = OUT_BUF_SIZE;
    }
}

extern "C" fn empty_output_buffer<W: Write>(cinfo: &mut jpeg_compress_struct) -> boolean {
    let cdata = unsafe { &mut *(*(cinfo.common.client_data as *mut cell::UnsafeCell<EncClientData<W>>)).get() };
    let writer: *mut W = &mut cdata.writer;
    if let Ok(()) = unsafe { (*writer).write_all(&cdata.buffer[0..OUT_BUF_SIZE-(*cinfo.dest).free_in_buffer]) } {
        unsafe {
            (*cinfo.dest).next_output_byte = cdata.buffer.as_mut_ptr();
            (*cinfo.dest).free_in_buffer = OUT_BUF_SIZE;
        }
    }
    true as boolean
}

extern "C" fn term_destination<W: Write>(cinfo: &mut jpeg_compress_struct) {
    empty_output_buffer::<W>(cinfo);
}

impl<W: Write> MozJPEGEncoder<W> {
    pub fn for_writer(w: W) -> MozJPEGEncoder<W> {
        let fd = unsafe { libc::fdopen(0, ffi::CString::new("wb").unwrap().as_ptr()) };
        let mut enc = MozJPEGEncoder::new(w);
        unsafe {
            jpeg_stdio_dest(&mut enc.cinfo, fd);
            (*enc.cinfo.dest).init_destination = Some(init_destination::<W>);
            (*enc.cinfo.dest).empty_output_buffer = Some(empty_output_buffer::<W>);
            (*enc.cinfo.dest).term_destination = Some(term_destination::<W>);
        }
        enc
    }
}
*/

impl<R> MozJPEGEncoder<R> {
    fn new(w: R) -> MozJPEGEncoder<R> {
        let mut enc = MozJPEGEncoder {
            cdata: cell::UnsafeCell::new(EncClientData {
                writer: w,
                buffer: [0; OUT_BUF_SIZE],
            }),
            err: unsafe { mem::zeroed() },
            cinfo: unsafe { mem::zeroed() },
            cleanup: None,
            quality: 85,
            jpgcrush: true,
            trellis: true,
            trellis_dc: true,
            trellis_eob_opt: true,
            trellis_q_opt: true,
            trellis_scans: true,
            deringing: true,
        };
        let size: size_t = mem::size_of_val(&enc.cinfo);
        enc.cinfo.common.err = unsafe { jpeg_std_error(&mut enc.err) };
        unsafe {
            jpeg_CreateCompress(&mut enc.cinfo, JPEG_LIB_VERSION, size);
        }
        enc.cinfo.common.client_data = &mut enc.cdata as *mut _ as *mut c_void;
        enc
    }

    pub fn encode(&mut self, image: &[u8], width: u32, height: u32, c: ColorType) -> bool {
        self.cinfo.image_width = width;
        self.cinfo.image_height = height;
        match c {
            Gray(n) => {
                self.cinfo.input_components = n as i32 / 8;
                self.cinfo.in_color_space = J_COLOR_SPACE::JCS_GRAYSCALE;
            },
            RGB(n) => {
                self.cinfo.input_components = n as i32 / 8 * 3;
                self.cinfo.in_color_space = J_COLOR_SPACE::JCS_RGB;
            },
            RGBA(n) => {
                self.cinfo.input_components = n as i32 / 8 * 4;
                self.cinfo.in_color_space = J_COLOR_SPACE::JCS_EXT_RGBA;
            },
            _ => return false,
        }
        unsafe {
            jpeg_set_defaults(&mut self.cinfo);
            self.cinfo.dct_method = J_DCT_METHOD::JDCT_ISLOW;
            jpeg_c_set_bool_param(&mut self.cinfo, JBOOLEAN_OPTIMIZE_SCANS, self.jpgcrush as boolean);
            if self.jpgcrush {
                jpeg_simple_progression(&mut self.cinfo);
            }
            jpeg_c_set_bool_param(&mut self.cinfo, JBOOLEAN_TRELLIS_QUANT, self.trellis as boolean);
            jpeg_c_set_bool_param(&mut self.cinfo, JBOOLEAN_TRELLIS_QUANT_DC, self.trellis_dc as boolean);
            jpeg_c_set_bool_param(&mut self.cinfo, JBOOLEAN_TRELLIS_EOB_OPT, self.trellis_eob_opt as boolean);
            jpeg_c_set_bool_param(&mut self.cinfo, JBOOLEAN_TRELLIS_Q_OPT, self.trellis_q_opt as boolean);
            jpeg_c_set_bool_param(&mut self.cinfo, JBOOLEAN_USE_SCANS_IN_TRELLIS, self.trellis_scans as boolean);
            jpeg_c_set_bool_param(&mut self.cinfo, JBOOLEAN_OVERSHOOT_DERINGING, self.deringing as boolean);
            jpeg_set_quality(&mut self.cinfo, self.quality as c_int, true as boolean);
            jpeg_start_compress(&mut self.cinfo, true as boolean);
            let stride = self.cinfo.image_width as isize * self.cinfo.input_components as isize;
            let mut bufp = image.as_ptr() as *const u8;
            while self.cinfo.next_scanline < self.cinfo.image_height {
                let sl = self.cinfo.next_scanline as isize;
                bufp = image.as_ptr().offset(sl * stride);
                jpeg_write_scanlines(&mut self.cinfo, &bufp, 1);
            }
            jpeg_finish_compress(&mut self.cinfo);
        };
        true
    }
}

impl<R> Drop for MozJPEGEncoder<R> {
    #[inline]
    fn drop(&mut self) {
        unsafe { jpeg_destroy_compress(&mut self.cinfo) }
        if let Some(f) = self.cleanup {
            f(self)
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::*;
    use std::io::Read;

    fn tests_for_example_jpg<R>(dec: &mut MozJPEGDecoder<R>) {
        assert_eq!(dec.raw_metadata().first().unwrap(), &(MetadataType::Comment, &b"Created with GIMP"[..]));
        assert_eq!(dec.dimensions().unwrap(), (4, 4));
        assert_eq!(dec.colortype().unwrap(), ColorType::RGB(8));
        assert_eq!(dec.row_len().unwrap(), 4 * 3);
        if let DecodingResult::U8(img) = dec.read_image().unwrap() {
            assert_eq!(img[0..3], [232, 193, 238]);
        } else {
            panic!("wtf");
        }
    }

    #[test]
    fn test_decode_file() {
        let mut dec = MozJPEGDecoder::for_file(File::open("fixtures/example.jpg").unwrap());
        tests_for_example_jpg(&mut dec);
    }

    #[test]
    fn test_decode_mem() {
        let mut vc = Vec::new();
        File::open("fixtures/example.jpg").unwrap().read_to_end(&mut vc).unwrap();
        let mut dec = MozJPEGDecoder::for_slice(&vc);
        tests_for_example_jpg(&mut dec);
    }

}
