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

#[repr(C, packed)]
pub struct AttributeHeader {
    pub type_id: u32,        // 0x00
    pub length: u32,         // 0x04
    pub non_resident: u8,    // 0x08
    pub name_length: u8,     // 0x09
    pub name_offset: u16,    // 0x0A
    pub flags: u16,          // 0x0C
    pub attribute_id: u16,   // 0x0E
    pub content_length: u32, // 0x10 (Resident Headers from here)
    pub content_offset: u16, // 0x14
    pub indexed_flag: u8,    // 0x16
    pub padding: u8,         // 0x17
}

fn get_filename_attribute(data: Vec<u8>, start_offset: u32) {
    let mut current_offset: usize = start_offset as usize;

    while current_offset + std::mem::size_of::<AttributeHeader>() <= data.len() {
        let header: &AttributeHeader =
            unsafe { &*(data.as_ptr().add(current_offset) as *const AttributeHeader) };

        match header.type_id {
            0xFFFFFFFF => break,
            0x30 => {
                println!("Found $FILE_NAME at offset {}", current_offset);
            }
            _ => (), // ignore other attribs
        }

        if header.length == 0 {
            break;
        }

        current_offset += header.length as usize;
    }
}

pub fn open_volume_handle() -> Result<HANDLE, String> {
    println!("Opening drive ...");

    let drive_handle = unsafe {
        CreateFileW(
            HANDLE_PATH,
            GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .map_err(|e| format!("OS Error: {}", e))?;

    if drive_handle.is_invalid() {
        return Err("Drive handle is invalid (Unknown Reason)".to_string());
    }

    // prepare a "bucket" to catch the data windows sends back
    let mut volume_data = NTFS_VOLUME_DATA_BUFFER::default();
    let mut bytes_returned = 0u32;

    unsafe {
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
    }
    .map_err(|e| format!("IOCTL Failed : {}", e))?;

    let mft_offset = volume_data.MftStartLcn * volume_data.BytesPerCluster as i64;

    println!("MFT Location Found!");
    println!("- Start Cluster: {}", volume_data.MftStartLcn);
    println!("- Bytes per Cluster: {}", volume_data.BytesPerCluster);
    println!("- Physical Byte Offset: {}", mft_offset);

    println!("!=- Found MFT Location. Pointing drive handle to byte-location of MFT.");

    unsafe {
            SetFilePointerEx(
        drive_handle,
        // We just provide mft_offset which is how much we need to move,
        // and by using FILE_CURRENT it will automatically find the
        // current position of the handle and move.
        mft_offset,
        None,
        FILE_CURRENT,
    )
    }
    .map_err(|e| format!("SetFilePointerEx Failed: {}", e))?;

    let mft_entry_size = volume_data.BytesPerFileRecordSegment;
    println!("MFT Entry Size: {}", mft_entry_size);
    println!("Drive Handle pointing to start of MFT. Beginning read[1x].");

    let mut read_data_buffer: Vec<u8> = vec![0u8; mft_entry_size as usize];
    let mut bytes_read: u32 = 0;

    unsafe {
        ReadFile(
        drive_handle,
        Some(&mut read_data_buffer[..]),
        Some(&mut bytes_read as *mut u32),
        None,
    )
    }
    .map_err(|e| format!("ReadFile Failed: {}", e))?;

    println!("Successfully read first record. Reading byte offset and then reading attributes ...");

    let header: &AttributeHeader = unsafe {
        &*(read_data_buffer.as_ptr() as *const AttributeHeader)
    };

    println!("Beginning attribute crawl");
    get_filename_attribute(read_data_buffer, header.content_offset as u32);

    return Ok(drive_handle);
}
