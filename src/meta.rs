

#[derive(Debug, PartialEq)]
pub enum MetadataType {
    Comment,
    ExifXmp,
    Iptc,
    Icc,
    Unknown,
}

pub trait MetadataDecoder {
    fn raw_metadata(&mut self) -> Vec<(MetadataType, &[u8])>;
}
