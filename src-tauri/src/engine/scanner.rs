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

        let mut read_data_buffer = vec![0u8; mft_entry_size as usize];
        let mut bytes_read: u32 = 0;

        ReadFile(
            drive_handle,
            Some(&mut read_data_buffer[..]),
            Some(&mut bytes_read as *mut u32),
            None,
        )
        .map_err(|e| format!("ReadFile Failed: {}", e))?;

        // DEBUG CODE TO SEE THE DATA READ
        /* 
        let actual_data = &read_data_buffer[..bytes_read as usize];

        println!("Bytes read (Hex):");
        for (i, byte) in actual_data.iter().enumerate() {
            // Print the byte as 2-character hex with a leading zero if needed
            print!("{:02X} ", byte);

            // Optional: Add a newline every 16 bytes for readability (hexdump style)
            if (i + 1) % 16 == 0 {
                println!();
            }
        }
        println!(); // Final newline
        */
        print!("Successfully read first record. Reading Byte Offset ...");
        

        return Ok(drive_handle);
    }
}
