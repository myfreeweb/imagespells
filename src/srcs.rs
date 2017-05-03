use std::fs;

pub trait DecoderFromFile {
    fn for_file(r: fs::File) -> Self;
}

pub trait DecoderFromMemory<'a> {
    fn for_slice(r: &'a [u8]) -> Self;
}

// TODO: from BufRead/Read
