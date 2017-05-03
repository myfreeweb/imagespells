# ImageSpells  [![unlicense](https://img.shields.io/badge/un-license-green.svg?style=flat)](http://unlicense.org)

**Work in progress**

A Rust library and a binary executable designed to replace ImageMagick for doing common web image manipulation tasks, based on [image](https://github.com/PistonDevelopers/image).

- [x] Fast (SIMD-accelerated) JPEG decoding with MozJPEG
- [ ] JPEG resize-on-decode
- [x] EXIF metadata extraction from JPEGs
- [ ] Fix EXIF stuff (ISO and Unicode) like exiv2 does
- [x] Optimized JPEG encoding with MozJPEG
- [ ] Optimized PNG encoding with oxipng (includes Zopfli compression)
- [ ] WebP decoding with libwebp
- [ ] WebP resize-on-decode
- [ ] WebP encoding with libwebp
- [ ] Watermarking
- [ ] Declarative manipulation pipeline
- [ ] Binary with sandboxing


## Usage

### As a binary

### As a library

```rust
extern crate imagespells;
// TODO
```

## Contributing

Please feel free to submit pull requests!

By participating in this project you agree to follow the [Contributor Code of Conduct](http://contributor-covenant.org/version/1/4/) and to release your contributions under the Unlicense.

[The list of contributors is available on GitHub](https://github.com/myfreeweb/secstr/graphs/contributors).

## License

This is free and unencumbered software released into the public domain.  
For more information, please refer to the `UNLICENSE` file or [unlicense.org](http://unlicense.org).
