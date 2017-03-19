use chunks::{Chunk, ChunkHeader};
use std::io::Cursor;
use byteorder::{LittleEndian, ReadBytesExt};
use std::rc::Rc;
use errors::*;
use std::fmt::{Display, Formatter};
use std::result::Result as StdResult;
use std::fmt::Error as FmtError;
use model::StringTable;
use encoding::codec::{utf_16, utf_8};
use model::owned::{StringTableBuf, Encoding as EncodingType};
use std::cell::RefCell;

pub struct StringTableDecoder;

impl StringTableDecoder {
    pub fn decode<'a>(cursor: &mut Cursor<&'a [u8]>, header: &ChunkHeader) -> Result<Chunk<'a>> {
        let stw = StringTableWrapper::new(cursor.get_ref(), *header);

        Ok(Chunk::StringTable(stw))
    }
}

pub struct StringTableWrapper<'a> {
    raw_data: &'a [u8],
    header: ChunkHeader,
}

impl<'a> StringTableWrapper<'a> {
    pub fn new(raw_data: &'a [u8], header: ChunkHeader) -> Self {
        StringTableWrapper {
            raw_data: raw_data,
            header: header,
        }
    }

    pub fn get_flags(&self) -> u32 {
        let mut cursor = Cursor::new(self.raw_data);
        cursor.set_position(self.header.absolute(16));

        cursor.read_u32::<LittleEndian>().unwrap_or(0)
    }

    pub fn to_owned(self) -> Result<StringTableBuf> {
        let mut owned = StringTableBuf::default();

        if !self.is_utf8() {
            owned.set_encoding(EncodingType::Utf16);
        }

        for i in 0..self.get_strings_len() {
            let ref string = *self.get_string(i)?;
            owned.add_string(string.clone());
        }

        Ok(owned)
    }

    fn get_string_position(&self, idx: u32) -> Result<u64> {
        let mut cursor = Cursor::new(self.raw_data);
        cursor.set_position(self.header.absolute(20));
        let str_offset = self.header.get_offset() as u32 + cursor.read_u32::<LittleEndian>()?;

        cursor.set_position(self.header.absolute(28));

        let mut position = str_offset;
        let mut max_offset = 0;

        for _ in 0..(idx + 1) {
            let current_offset = cursor.read_u32::<LittleEndian>()?;
            position = str_offset + current_offset;

            if current_offset > max_offset {
                max_offset = current_offset
            }
        }

        Ok(position as u64)
    }

    fn parse_string(&self, offset: u32) -> Result<String> {
        let mut cursor = Cursor::new(self.raw_data);
        cursor.set_position(offset as u64);

        if self.is_utf8() {
            let mut ini_offset = offset;
            let v = cursor.read_u8()? as u32;
            if v == 0x80 {
                ini_offset += 2;
                cursor.read_u8()?;
            } else {
                ini_offset += 1;
            }

            let v = cursor.read_u8()? as u32;
            if v == 0x80 {
                ini_offset += 2;
                cursor.read_u8()?;
            } else {
                ini_offset += 1;
            }

            let mut length = 0;

            loop {
                let v = cursor.read_u8()?;

                if v != 0 {
                    length += 1;
                } else {
                    break;
                }
            }

            let a = ini_offset;
            let b = ini_offset + length;

            if a > self.raw_data.len() as u32 || b > self.raw_data.len() as u32 || a > b {
                return Err("Sub-slice out of raw_data range".into());
            }

            let subslice: &[u8] = &self.raw_data[a as usize..b as usize];

            let mut decoder = utf_8::UTF8Decoder::new();
            let mut o = String::new();
            decoder.raw_feed(subslice, &mut o);
            let decode_error = decoder.raw_finish(&mut o);

            match decode_error {
                None => Ok(o),
                Some(_) => Err("Error decoding UTF8 string".into()),
            }
        } else {
            let size1 = cursor.read_u8()? as u32;
            let size2 = cursor.read_u8()? as u32;

            let val = ((size2 & 0xFF) << 8) | size1 & 0xFF;

            let a = offset + 2;
            let b = offset + 2 + (val * 2);


            if a > self.raw_data.len() as u32 || b > self.raw_data.len() as u32 || a > b {
                return Err("Sub-slice out of raw_data range".into());
            }

            let subslice: &[u8] = &self.raw_data[a as usize..b as usize];

            let mut decoder = utf_16::UTF16Decoder::<utf_16::Little>::new();
            let mut o = String::new();
            decoder.raw_feed(subslice, &mut o);
            let decode_error = decoder.raw_finish(&mut o);

            match decode_error {
                None => Ok(o),
                Some(_) => Err("Error decoding UTF16 string".into()),
            }
        }
    }

    fn is_utf8(&self) -> bool {
        (self.get_flags() & 0x00000100) == 0x00000100
    }
}

impl<'a> Display for StringTableWrapper<'a> {
    fn fmt(&self, formatter: &mut Formatter) -> StdResult<(), FmtError> {
        let amount = self.get_strings_len();

        for i in 0..amount {
            let current_string = self.get_string(i).unwrap_or(Rc::new("<UNKOWN>".to_string()));

            write!(formatter, "{} - {}\n", i, current_string)?;
        }

        Ok(())
    }
}

impl<'a> StringTable for StringTableWrapper<'a> {
    fn get_strings_len(&self) -> u32 {
        let mut cursor = Cursor::new(self.raw_data);
        cursor.set_position(self.header.absolute(8));

        cursor.read_u32::<LittleEndian>().unwrap_or(0)
    }

    fn get_styles_len(&self) -> u32 {
        let mut cursor = Cursor::new(self.raw_data);
        cursor.set_position(self.header.absolute(12));

        cursor.read_u32::<LittleEndian>().unwrap_or(0)
    }

    fn get_string(&self, idx: u32) -> Result<Rc<String>> {
        if idx > self.get_strings_len() {
            return Err("Index out of bounds".into());
        }

        let string = self.get_string_position(idx)
            .and_then(|position| self.parse_string(position as u32))?;

        Ok(Rc::new(string))
    }
}

pub struct CountingStringTable<S: StringTable> {
    inner: S,
    counters: RefCell<Vec<u32>>,
}

impl<S: StringTable> CountingStringTable<S> {
    pub fn new(inner: S) -> Self {
        let str_amount = inner.get_strings_len();

        CountingStringTable {
            inner: inner,
            counters: RefCell::new(vec![0; str_amount as usize]),
        }
    }

    pub fn display_stats(&self) {
        let counter_borrow = self.counters.borrow();
        let mut new_counters: Vec<u32> = counter_borrow.clone();
        let amount = new_counters.len();
        new_counters.sort();

        println!("Sorted: {:?}", new_counters);
    }
}

impl<S: StringTable> StringTable for CountingStringTable<S> {
    fn get_strings_len(&self) -> u32 {
        self.inner.get_strings_len()
    }

    fn get_styles_len(&self) -> u32 {
        self.inner.get_styles_len()
    }

    fn get_string(&self, idx: u32) -> Result<Rc<String>> {
        let mut count_borrow = self.counters.borrow_mut();
        count_borrow[idx as usize] += 1;

        self.inner.get_string(idx)
    }
}
