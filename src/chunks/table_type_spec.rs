use chunks::{Chunk, ChunkHeader};
use std::io::Cursor;
use byteorder::{LittleEndian, ReadBytesExt};
use errors::*;
use model::TypeSpec as TypeSpecTrait;
use model::owned::TableTypeSpecBuf;

pub struct TableTypeSpecDecoder;

impl TableTypeSpecDecoder {
    pub fn decode<'a>(cursor: &mut Cursor<&'a [u8]>, header: &ChunkHeader) -> Result<Chunk<'a>> {
        let tsw = TypeSpecWrapper::new(cursor.get_ref(), *header);
        Ok(Chunk::TableTypeSpec(tsw))
    }
}

#[derive(Clone)]
pub struct TypeSpecWrapper<'a> {
    raw_data: &'a [u8],
    header: ChunkHeader,
}

impl<'a> TypeSpecWrapper<'a> {
    pub fn new(raw_data: &'a [u8], header: ChunkHeader) -> Self {
        TypeSpecWrapper {
            raw_data: raw_data,
            header: header,
        }
    }

    pub fn get_id(&self) -> Result<u32> {
        let mut cursor = Cursor::new(self.raw_data);
        cursor.set_position(self.header.absolute(8));

        Ok(cursor.read_u32::<LittleEndian>()?)
    }

    pub fn get_amount(&self) -> Result<u32> {
        let mut cursor = Cursor::new(self.raw_data);
        cursor.set_position(self.header.absolute(12));

        Ok(cursor.read_u32::<LittleEndian>()?)
    }

    pub fn get_flag(&self, index: u32) -> Result<u32> {
        let amount = self.get_amount()?;

        if index >= amount {
            return Err(format!("Invalid flag on index {} out of {}", index, amount).into());
        }

        let mut cursor = Cursor::new(self.raw_data);
        let flag_offset = 16 + (index * 4) as u64;
        cursor.set_position(self.header.absolute(flag_offset));

        Ok(cursor.read_u32::<LittleEndian>()?)
    }

    pub fn to_owned(self) -> Result<TableTypeSpecBuf> {
        let mut owned = TableTypeSpecBuf::new(self.get_id()? as u16);
        let amount = self.get_amount()?;

        for i in 0..amount {
            owned.push_flag(self.get_flag(i)?);
        }

        Ok(owned)
    }
}

#[derive(Clone)]
pub struct TypeSpec<'a> {
    wrapper: TypeSpecWrapper<'a>,
}

impl<'a> TypeSpec<'a> {
    pub fn new(wrapper: TypeSpecWrapper<'a>) -> Self {
        TypeSpec { wrapper: wrapper }
    }
}

impl<'a> TypeSpecTrait for TypeSpec<'a> {
    fn get_id(&self) -> Result<u16> {
        Ok((self.wrapper.get_id()? & 0xFF) as u16)
    }

    fn get_amount(&self) -> Result<u32> {
        self.wrapper.get_amount()
    }

    fn get_flag(&self, index: u32) -> Result<u32> {
        self.wrapper.get_flag(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raw_chunks;
    use chunks::ChunkHeader;
    use model::owned::OwnedBuf;

    #[test]
    fn it_can_decode_a_type_spec() {
        let header = ChunkHeader::new(0, 16, raw_chunks::EXAMPLE_TYPE_SPEC.len() as u32, 0x202);
        let wrapper = TypeSpecWrapper::new(raw_chunks::EXAMPLE_TYPE_SPEC, header);

        assert_eq!(4, wrapper.get_id().unwrap());
        assert_eq!(1541, wrapper.get_amount().unwrap());

        assert_eq!(0x40000004, wrapper.get_flag(0).unwrap());
        assert_eq!(0, wrapper.get_flag(25).unwrap());
        assert_eq!(6, wrapper.get_flag(1540).unwrap());

        let errored_flag = wrapper.get_flag(1541);
        assert!(errored_flag.is_err());
        assert_eq!("Invalid flag on index 1541 out of 1541", errored_flag.err().unwrap().to_string());
    }

    #[test]
    fn identity() {
        let header = ChunkHeader::new(0, 16, raw_chunks::EXAMPLE_TYPE_SPEC.len() as u32, 0x202);
        let wrapper = TypeSpecWrapper::new(raw_chunks::EXAMPLE_TYPE_SPEC, header);

        let owned = wrapper.to_owned().unwrap();
        let new_raw = owned.to_vec().unwrap();

        assert_eq!(&new_raw, &raw_chunks::EXAMPLE_TYPE_SPEC);
    }
}