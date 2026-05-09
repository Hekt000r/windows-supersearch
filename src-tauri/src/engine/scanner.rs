use std::ptr::read_unaligned;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, GENERIC_READ};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Ioctl::{
    FSCTL_ENUM_USN_DATA, FSCTL_QUERY_USN_JOURNAL, MFT_ENUM_DATA_V0, USN_JOURNAL_DATA_V0,
};
use windows::Win32::System::IO::DeviceIoControl;

const HANDLE_PATH: PCWSTR = windows::core::w!("\\\\.\\C:"); // \\.\C:

pub fn scan_volume() -> anyhow::Result<()> {
    let handle = unsafe {
        CreateFileW(
            HANDLE_PATH,
            GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )?
    };

    let mut journal = USN_JOURNAL_DATA_V0::default();
    let _journal_update_result = unsafe {
        USN_JOURNAL_DATA_V0::default();
        DeviceIoControl(
            handle,
            FSCTL_QUERY_USN_JOURNAL,
            None,
            0,
            Some(&mut journal as *mut _ as *mut _),
            size_of::<USN_JOURNAL_DATA_V0>() as u32,
            None,
            None,
        )
    };


    let mut enum_data = MFT_ENUM_DATA_V0 {
        StartFileReferenceNumber: 0,
        LowUsn: 0,
        HighUsn: journal.NextUsn,
    };

    let mut buffer = vec![0u8; 1024 * 1024];
    let mut total_files = 0;

    loop {
        let mut bytes_returned = 0;
        let ok = unsafe {
            DeviceIoControl(
                handle,
                FSCTL_ENUM_USN_DATA,
                Some(&enum_data as *const _ as *const _),
                size_of::<MFT_ENUM_DATA_V0>() as u32,
                Some(buffer.as_mut_ptr() as *mut _),
                buffer.len() as u32,
                Some(&mut bytes_returned),
                None,
            )
        };

        if ok.is_err() || bytes_returned < 8 {
            break;
        }

        enum_data.StartFileReferenceNumber =
            unsafe { read_unaligned(buffer.as_ptr() as *const u64) };

        let mut cursor = 8;

        while cursor < bytes_returned as usize {
            let ptr = unsafe { buffer.as_ptr().add(cursor) };

            let (record_len, file_id, parent_id, name_len, name_off) = unsafe {
                (
                    read_unaligned(ptr as *const u32) as usize,
                    read_unaligned(ptr.add(8) as *const u64),
                    read_unaligned(ptr.add(16) as *const u64),
                    read_unaligned(ptr.add(56) as *const u16) as usize,
                    read_unaligned(ptr.add(58) as *const u16) as usize,
                )
            };

            let name_ptr: *const u16 = unsafe {
                ptr.add(name_off) as *const u16
            };
            let name_slice: &[u16] = unsafe { std::slice::from_raw_parts(name_ptr, name_len / 2) };
            let name: String = String::from_utf16_lossy(name_slice);

            total_files += 1;

            cursor += record_len;
        }

    }
    unsafe {CloseHandle(handle)?};
    println!("Processed {} files.", total_files);

    Ok(())
}
