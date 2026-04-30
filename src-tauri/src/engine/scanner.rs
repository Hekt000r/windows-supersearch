use std::mem;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{GENERIC_READ, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, SetFilePointerEx, FILE_CURRENT, FILE_FLAGS_AND_ATTRIBUTES,
    FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Ioctl::{FSCTL_GET_NTFS_VOLUME_DATA, NTFS_VOLUME_DATA_BUFFER};
use windows::Win32::System::IO::DeviceIoControl;

const HANDLE_PATH: PCWSTR = windows::core::w!("\\\\.\\C:"); // \\.\C:

fn get_filename_attribute(data_buffer: Vec<u8>, start_offset: u32) {
    let mut is_crawling: bool = true;
    let mut crawl_count: u32 = 0;
    let mut current_offset: u32 = start_offset;

    while is_crawling {
            println!("Crawl no.{}", crawl_count);

            if crawl_count == 0 {
                // first time crawling this record
                current_offset = start_offset as u32;
            }

            let start: usize = current_offset as usize;
            let end: usize = (current_offset + 4) as usize;

            // read the type ID which is 4 bytes
            let type_id_bytes: &[u8] = &data_buffer[start..end];
            let type_id: u32 = u32::from_le_bytes(type_id_bytes.try_into().unwrap());
            if type_id == 0xFFFFFFFF {
                // NTFS STOP telling us that there are no more attributes
                // so reset the crawl count and break.
                crawl_count = 0;
                is_crawling = false;
                break;
            }

            // now we need the length of this attribute (stored in bytes 4-8)
            // we have the start of the attribute and the end of it
            // so attribute_length = take the next 4 bytes from start_of_attr
            let attr_len_bytes_start: usize = (current_offset + 4) as usize;
            let attr_len_bytes_end: usize = (current_offset + 8) as usize;
            let attr_len_bytes: &[u8] = &data_buffer[attr_len_bytes_start..attr_len_bytes_end];
            let attr_len: u32 = u32::from_le_bytes(attr_len_bytes.try_into().unwrap());

            // if corrupted data
            if attr_len == 0 {
                crawl_count = 0;
                is_crawling = false;
                break;
            }

            // now we can find the attribute we're looking for
            // for testing purposes lets find filename (0x30)
            if type_id == 0x30 {
                println!("Filename attribute found");
                // we need to know where the content is stored
                // the offset to content is stored in bytes 0x14 and 0x15
                // TODO: replace all the weird "_bytes" bullshit with this much cleaner syntax
                let filename_content_offset: u16 = u16::from_le_bytes([
                    data_buffer[start + 0x14],
                    data_buffer[start + 0x15]
                ]);
                println!("$FILE_NAME content offset: {}", filename_content_offset);

                // instead of "jumping" the current_offset var
                // which could have weird consenquences we will just create new variables
                // since the offset is relative to the header, and type_id is at 0x00 of the header
                // we can just use type_id's row number ("start" variable) for this calculation
                let file_name_struct_start_row: u32 = (start as u32 + filename_content_offset as u32);
                println!("$FILE_NAME struct start row: {}", file_name_struct_start_row);
                // now we need to find the filename length, which is located
                // at offset 0x40 (64) from $FILE_NAME struct start
                let file_name_byte_location: usize = (file_name_struct_start_row + 0x40) as usize;
                println!("$FILE_NAME byte location: {}", file_name_byte_location);

                let file_name_length: u8 = u8::from_le_bytes([
                    data_buffer[file_name_byte_location]
                ]);
                println!("$FILE_NAME length found: {}",file_name_length);
            }

            current_offset += attr_len;
            crawl_count += 1;
        }    

}

pub fn open_volume_handle() -> Result<HANDLE, String> {
    println!("Opening drive ...");
    unsafe {
        let drive_handle = CreateFileW(
            HANDLE_PATH,
            GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
        .map_err(|e| format!("OS Error: {}", e))?;

        if drive_handle.is_invalid() {
            return Err("Drive handle is invalid (Unknown Reason)".to_string());
        }

        // prepare a "bucket" to catch the data windows sends back
        let mut volume_data = NTFS_VOLUME_DATA_BUFFER::default();
        let mut bytes_returned = 0u32;

        DeviceIoControl(
            drive_handle,
            FSCTL_GET_NTFS_VOLUME_DATA,
            None,
            0,
            Some(&mut volume_data as *mut _ as *mut _),
            mem::size_of::<NTFS_VOLUME_DATA_BUFFER>() as u32,
            Some(&mut bytes_returned),
            None,
        )
        .map_err(|e| format!("IOCTL Failed : {}", e))?;

        let mft_offset = volume_data.MftStartLcn * volume_data.BytesPerCluster as i64;

        println!("MFT Location Found!");
        println!("- Start Cluster: {}", volume_data.MftStartLcn);
        println!("- Bytes per Cluster: {}", volume_data.BytesPerCluster);
        println!("- Physical Byte Offset: {}", mft_offset);

        println!("!=- Found MFT Location. Pointing drive handle to byte-location of MFT.");

        SetFilePointerEx(
            drive_handle,
            // We just provide mft_offset which is how much we need to move,
            // and by using FILE_CURRENT it will automatically find the
            // current position of the handle and move.
            mft_offset,
            None,
            FILE_CURRENT,
        )
        .map_err(|e| format!("SetFilePointerEx Failed: {}", e))?;

        let mft_entry_size = volume_data.BytesPerFileRecordSegment;
        println!("MFT Entry Size: {}", mft_entry_size);
        println!("Drive Handle pointing to start of MFT. Beginning read[1x].");

        let mut read_data_buffer: Vec<u8> = vec![0u8; mft_entry_size as usize];
        let mut bytes_read: u32 = 0;

        ReadFile(
            drive_handle,
            Some(&mut read_data_buffer[..]),
            Some(&mut bytes_read as *mut u32),
            None,
        )
        .map_err(|e| format!("ReadFile Failed: {}", e))?;

        println!(
            "Successfully read first record. Reading byte offset and then reading attributes ..."
        );

        let first_attribute_offset =
            u16::from_le_bytes([read_data_buffer[0x14], read_data_buffer[0x15]]) as usize;

        let first_attribute: &[u8] = &read_data_buffer[first_attribute_offset as usize..];
        let first_attr_total_len: u32 =
            u32::from_le_bytes(first_attribute[4..8].try_into().unwrap());



        println!("First attribute length: {}",first_attr_total_len);

        let attr_offset_bytes: &[u8] = &read_data_buffer[20..22];
        let z_attr_offset: u16 = u16::from_le_bytes(attr_offset_bytes.try_into().unwrap());

        println!("Beginning attribute crawl");
        get_filename_attribute(read_data_buffer, z_attr_offset as u32);

        return Ok(drive_handle);
    }
}
