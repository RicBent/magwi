use std::usize;
use std::collections::BTreeMap;

use super::error::*;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum HookExtraPos {
    Loader,
    Tail,
}

#[derive(Debug, PartialEq, Clone)]
pub enum HookWriteReason {
    Misc,
    _Code,
    _Loader,
    _Hook(Vec<super::HookLocation>),
}

pub struct HookWriter {
    base_address: u32,
    loader_extra_address: Option<u32>,
    buffer: Vec<u8>,
    duplicate_write_check: bool,
    write_reasons: BTreeMap<u32, (u32, HookWriteReason)>,
}

impl HookWriter {
    pub fn new(base_address: u32, buffer: Vec<u8>) -> Self {
        Self {
            base_address,
            loader_extra_address: None,
            buffer,
            duplicate_write_check: true,
            write_reasons: BTreeMap::new(),
        }
    }

    pub fn data(&self) -> &[u8] {
        &self.buffer
    }

    pub fn base_address(&self) -> u32 {
        self.base_address
    }

    pub fn end_address(&self) -> u32 {
        self.base_address + self.buffer.len() as u32
    }

    pub fn set_loader_extra_address(&mut self, address: u32) {
        self.loader_extra_address = Some(address);
    }

    pub fn read_mut(&self, address: u32, data: &mut [u8]) -> Result<(), WriterError> {
        if address < self.base_address {
            return Err(WriterError::OutOfBoundsRead(address, data.len()));
        }

        let offset = address as usize - self.base_address as usize;

        if offset + data.len() > self.buffer.len() {
            return Err(WriterError::OutOfBoundsRead(address, data.len()));
        }

        data.copy_from_slice(&self.buffer[offset..offset + data.len()]);

        Ok(())
    }

    #[inline]
    pub fn read<const COUNT: usize>(&self, address: u32) -> Result<[u8; COUNT], WriterError> {
        let mut data = [0; COUNT];
        self.read_mut(address, &mut data)?;
        Ok(data)
    }

    fn find_duplicate_write(&self, address: u32, size: u32) -> Option<&HookWriteReason> {
        let mut iter = self.write_reasons.range(..address + size);

        if let Some((check_address, (check_size, check_reason))) = iter.next_back() {
            if *check_address + *check_size as u32 > address {
                return Some(check_reason);
            }
        }

        None
    }

    pub fn write(&mut self, address: u32, data: impl AsRef<[u8]>) -> Result<(), WriterError> {
        let data = data.as_ref();

        if address < self.base_address {
            return Err(WriterError::OutOfBoundsWrite(address, data.len()));
        }

        let offset = address as usize - self.base_address as usize;

        if offset + data.len() > self.buffer.len() {
            return Err(WriterError::OutOfBoundsWrite(address, data.len()));
        }

        if self.duplicate_write_check {
            if let Some(_write_reason) = self.find_duplicate_write(address, data.len() as u32) {
                return Err(WriterError::DuplicateWrite(address, data.len()));
            }
        }

        self.buffer[offset..offset + data.as_ref().len()].copy_from_slice(data.as_ref());
        self.write_reasons.insert(address, (data.len() as u32, HookWriteReason::Misc));

        Ok(())
    }

    pub fn write_end(&mut self, data: impl AsRef<[u8]>) -> Result<(), WriterError> {
        self.buffer.extend_from_slice(data.as_ref());
        Ok(())
    }

    pub fn write_extra<F: FnOnce(&mut HookWriter, &mut HookWriter) -> ()>(
        &mut self,
        pos: HookExtraPos,
        write_fn: F,
    ) -> Result<(), WriterError> {
        let address = match pos {
            HookExtraPos::Loader => self
                .loader_extra_address
                .ok_or(WriterError::LoaderExtraAddressNotSet)?,
            HookExtraPos::Tail => self.base_address + self.buffer.len() as u32,
        };

        let mut w = HookWriter::new(address, Vec::new());
        write_fn(self, &mut w);

        let data = w.buffer;

        match pos {
            HookExtraPos::Loader => {
                self.write(address, &data)?;
                self.loader_extra_address = Some(address + data.len() as u32);
            }
            HookExtraPos::Tail => self.write_end(&data)?,
        }

        Ok(())
    }

    pub fn resize_until(&mut self, until_address: u32) -> Result<(), WriterError> {
        if until_address < self.base_address {
            return Err(WriterError::ResizeBelowBaseAddress(until_address));
        }

        let buf_size = until_address as usize - self.base_address as usize;
        self.buffer.resize(buf_size, 0);

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_read() {
        let writer = HookWriter::new(
            0x1000,
            vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08],
        );

        assert_eq!(writer.read::<1>(0x1000).unwrap(), [0x00]);
        assert_eq!(writer.read::<2>(0x1000).unwrap(), [0x00, 0x01]);
        assert_eq!(writer.read::<4>(0x1000).unwrap(), [0x00, 0x01, 0x02, 0x03]);

        assert_eq!(0x00, u8::from_le_bytes(writer.read(0x1000).unwrap()));
        assert_eq!(0x0100, u16::from_le_bytes(writer.read(0x1000).unwrap()));
        assert_eq!(0x03020100, u32::from_le_bytes(writer.read(0x1000).unwrap()));

        assert_eq!(writer.read::<1>(0x1001).unwrap(), [0x01]);
        assert_eq!(writer.read::<4>(0x1005).unwrap(), [0x05, 0x06, 0x07, 0x08]);

        assert_eq!(
            writer.read::<1>(0x0FFF).unwrap_err(),
            WriterError::OutOfBoundsRead(0x0FFF, 1)
        );
        assert_eq!(
            writer.read::<2>(0x0FFF).unwrap_err(),
            WriterError::OutOfBoundsRead(0x0FFF, 2)
        );
        assert_eq!(
            writer.read::<1>(0x1009).unwrap_err(),
            WriterError::OutOfBoundsRead(0x1009, 1)
        );
        assert_eq!(
            writer.read::<2>(0x1008).unwrap_err(),
            WriterError::OutOfBoundsRead(0x1008, 2)
        );
        assert_eq!(
            writer.read::<11>(0x0FFF).unwrap_err(),
            WriterError::OutOfBoundsRead(0x0FFF, 11)
        );
    }

    #[test]
    fn test_write() {
        let mut writer = HookWriter::new(0x1000, vec![0x00; 4]);

        writer.write(0x1000, &[0x01]).unwrap();
        assert_eq!(writer.read::<1>(0x1000).unwrap(), [0x01]);

        writer.write(0x1000, &[0x02, 0x03]).unwrap();
        assert_eq!(writer.read::<2>(0x1000).unwrap(), [0x02, 0x03]);

        writer.write(0x1000, &[0x04, 0x05, 0x06, 0x07]).unwrap();
        assert_eq!(writer.read::<4>(0x1000).unwrap(), [0x04, 0x05, 0x06, 0x07]);

        writer.write(0x1001, &[0x08, 0x09]).unwrap();
        assert_eq!(writer.read::<2>(0x1001).unwrap(), [0x08, 0x09]);

        assert_eq!(
            writer.write(0x0FFF, &[0x01]).unwrap_err(),
            WriterError::OutOfBoundsWrite(0x0FFF, 1)
        );

        assert_eq!(
            writer.write(0x0FFF, &[0x01, 0x02]).unwrap_err(),
            WriterError::OutOfBoundsWrite(0x0FFF, 2)
        );

        assert_eq!(
            writer.write(0x1004, &[0x01]).unwrap_err(),
            WriterError::OutOfBoundsWrite(0x1004, 1)
        );

        assert_eq!(
            writer.write(0x1003, &[0x01, 0x02]).unwrap_err(),
            WriterError::OutOfBoundsWrite(0x1003, 2)
        );

        assert_eq!(
            writer
                .write(0x1000, &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06])
                .unwrap_err(),
            WriterError::OutOfBoundsWrite(0x1000, 6)
        );
    }

    #[test]
    fn test_duplicate_write() {
        let mut writer = HookWriter::new(0x1000, vec![0x00; 4]);
        writer.write(0x1001, &[0x01; 2]).unwrap();

        assert_eq!(
            writer.write(0x1001, &[0x01]).unwrap_err(),
            WriterError::DuplicateWrite(0x1001, 1)
        );

        assert_eq!(
            writer.write(0x1002, &[0x01]).unwrap_err(),
            WriterError::DuplicateWrite(0x1002, 1)
        );

        assert_eq!(
            writer.write(0x1001, &[0x01, 0x02]).unwrap_err(),
            WriterError::DuplicateWrite(0x1001, 2)
        );

        assert_eq!(
            writer.write(0x1000, &[0x01, 0x02]).unwrap_err(),
            WriterError::DuplicateWrite(0x1000, 2)
        );
    }

    #[test]
    fn test_write_end() {
        let mut writer = HookWriter::new(0x1000, vec![0x00; 4]);
        writer.write_end(&[0x01]).unwrap();
        assert_eq!(
            writer.read::<5>(0x1000).unwrap(),
            [0x00, 0x00, 0x00, 0x00, 0x01]
        );
    }

    #[test]
    fn test_write_extra() {
        let mut writer = HookWriter::new(0x1000, vec![0x00; 0x6]);

        assert_eq!(
            writer
                .write_extra(HookExtraPos::Loader, |_, w| {
                    w.write_end(&[0x01]).unwrap();
                })
                .unwrap_err(),
            WriterError::LoaderExtraAddressNotSet
        );

        writer.set_loader_extra_address(0x1002);
        writer
            .write_extra(HookExtraPos::Loader, |_, w| {
                w.write_end(&[0x01, 0x02]).unwrap();
            })
            .unwrap();
        assert_eq!(
            writer.read::<6>(0x1000).unwrap(),
            [0x00, 0x00, 0x01, 0x02, 0x00, 0x00]
        );

        writer
            .write_extra(HookExtraPos::Tail, |_, w| {
                w.write_end(&[0x03, 0x04]).unwrap();
            })
            .unwrap();
        assert_eq!(
            writer.read::<8>(0x1000).unwrap(),
            [0x00, 0x00, 0x01, 0x02, 0x00, 0x00, 0x03, 0x04]
        );
    }

    #[test]
    fn test_resize_until() {
        let mut writer = HookWriter::new(0x1000, vec![0xAA; 4]);

        writer.resize_until(0x1008).unwrap();
        assert_eq!(
            writer.read::<8>(0x1000).unwrap(),
            [0xAA, 0xAA, 0xAA, 0xAA, 0x00, 0x00, 0x00, 0x00]
        );
        assert_eq!(
            writer.read::<1>(0x0FFF).unwrap_err(),
            WriterError::OutOfBoundsRead(0x0FFF, 1)
        );
        assert_eq!(
            writer.read::<1>(0x1008).unwrap_err(),
            WriterError::OutOfBoundsRead(0x1008, 1)
        );

        writer.resize_until(0x1004).unwrap();
        assert_eq!(writer.read::<4>(0x1000).unwrap(), [0xAA; 4]);
        assert_eq!(
            writer.read::<1>(0x0FFF).unwrap_err(),
            WriterError::OutOfBoundsRead(0x0FFF, 1)
        );
        assert_eq!(
            writer.read::<1>(0x1004).unwrap_err(),
            WriterError::OutOfBoundsRead(0x1004, 1)
        );

        assert_eq!(
            writer.resize_until(0x0FFF).unwrap_err(),
            WriterError::ResizeBelowBaseAddress(0x0FFF)
        );
        assert_eq!(writer.read::<4>(0x1000).unwrap(), [0xAA; 4]);
    }
}
