use exif;

#[derive(Debug, PartialEq)]
pub enum MetadataType {
    Comment,
    ExifXmp,
    Iptc,
    Icc,
    Unknown,
}

#[derive(Debug)]
pub enum Metadata<'a> {
    Comment(String),
    Exif(Vec<exif::Field<'a>>),
    Unsupported(MetadataType, Vec<u8>),
    DecodingError,
}

pub trait MetadataDecoder {
    fn raw_metadata(&mut self) -> Vec<(MetadataType, &[u8])>;

    fn parsed_metadata(&mut self) -> Vec<Metadata> {
        self.raw_metadata().into_iter().map(|(typ, dat)| {
            match typ {
                MetadataType::Comment =>
                    Metadata::Comment(String::from_utf8_lossy(dat).into_owned()),
                MetadataType::ExifXmp => {
                    if &dat[0..6] == b"Exif\0\0" {
                        match exif::parse_exif(&dat[6..]) {
                            Ok((m, _)) => Metadata::Exif(m),
                            Err(_) => Metadata::DecodingError,
                        }
                    } else {
                        Metadata::Unsupported(typ, dat.to_owned())
                    }
                },
                _ => Metadata::Unsupported(typ, dat.to_owned()),
            }
        }).collect()
    }
}
