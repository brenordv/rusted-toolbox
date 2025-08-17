use anyhow::{Context, Result};
use csv::{Reader, StringRecord, StringRecordsIter};
use memmap2::Mmap;
use std::fs::File;
use std::io::Cursor;
use std::path::PathBuf;

/// TODO: Add summary
pub struct MmapCsvReader {
    _mmap: Mmap,
    reader: Reader<Cursor<&'static [u8]>>,
}

impl MmapCsvReader {
    /// TODO: Add summary
    pub fn new(file: &PathBuf) -> Result<Self> {
        let file = File::open(file).context("Unable to open file for memory mapping")?;
        let mmap = unsafe { Mmap::map(&file).context("Unable to memory map file")? };

        // Convert to static lifetime while we hold the mmap
        let data: &'static [u8] = unsafe { std::mem::transmute(mmap.as_ref()) };
        let cursor = Cursor::new(data);
        let reader = Reader::from_reader(cursor);

        Ok(Self {
            _mmap: mmap,
            reader,
        })
    }

    /// TODO: Add summary
    pub fn headers(&mut self) -> csv::Result<&StringRecord> {
        self.reader.headers()
    }

    /// TODO: Add summary
    pub fn records(&mut self) -> StringRecordsIter<Cursor<&'static [u8]>> {
        self.reader.records()
    }
}
