//! A struct adapter of Read+Seek+Write to append to IPC files
// read header and convert to writer information
// seek to first byte of header - 1
// write new batch
// write new footer
use std::io::{Read, Seek, SeekFrom, Write};

use crate::error::{ArrowError, Result};

use super::endianess::is_native_little_endian;
use super::read::{self, FileMetadata};
use super::write::common::DictionaryTracker;
use super::write::writer::*;
use super::write::*;

impl<R: Read + Seek + Write> FileWriter<R> {
    /// Creates a new [`FileWriter`] from an existing file, seeking to the last message
    /// and appending new messages afterwards. Users call `finish` to write the footer (with both)
    /// the existing and appended messages on it.
    /// # Error
    /// This function errors iff:
    /// * the file's endianess is not the native endianess (not yet supported)
    /// * the file is not a valid Arrow IPC file
    pub fn try_from_file(
        mut writer: R,
        metadata: FileMetadata,
        options: WriteOptions,
    ) -> Result<FileWriter<R>> {
        if metadata.ipc_schema.is_little_endian != is_native_little_endian() {
            return Err(ArrowError::nyi(
                "Appending to a file of a non-native endianess is still not supported",
            ));
        }

        let dictionaries = if let Some(blocks) = &metadata.dictionaries {
            read::reader::read_dictionaries(
                &mut writer,
                &metadata.schema.fields,
                &metadata.ipc_schema,
                blocks,
            )?
        } else {
            Default::default()
        };

        let last_block = metadata.blocks.last().ok_or_else(|| {
            ArrowError::oos("An Arrow IPC file must have at least 1 message (the schema message)")
        })?;
        let offset: u64 = last_block
            .offset
            .try_into()
            .map_err(|_| ArrowError::oos("The block's offset must be a positive number"))?;
        let meta_data_length: u64 = last_block
            .meta_data_length
            .try_into()
            .map_err(|_| ArrowError::oos("The block's meta length must be a positive number"))?;
        let body_length: u64 = last_block
            .body_length
            .try_into()
            .map_err(|_| ArrowError::oos("The block's body length must be a positive number"))?;
        let offset: u64 = offset + meta_data_length + body_length;

        writer.seek(SeekFrom::Start(offset))?;

        Ok(FileWriter {
            writer,
            options,
            schema: metadata.schema,
            ipc_fields: metadata.ipc_schema.fields,
            block_offsets: offset as usize,
            dictionary_blocks: metadata.dictionaries.unwrap_or_default(),
            record_blocks: metadata.blocks,
            state: State::Started, // file already exists, so we are ready
            dictionary_tracker: DictionaryTracker {
                dictionaries,
                cannot_replace: true,
            },
        })
    }
}
