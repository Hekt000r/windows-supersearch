use std::{mem, os::raw::c_void};

use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{GENERIC_READ, HANDLE},
        Storage::FileSystem::{
            CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
            OPEN_EXISTING,
        },
        System::{
            Ioctl::{
                FSCTL_QUERY_USN_JOURNAL, FSCTL_READ_USN_JOURNAL, READ_USN_JOURNAL_DATA_V0,
                USN_JOURNAL_DATA_V2, USN_RECORD_V2,
            },
            IO::DeviceIoControl,
        },
    },
};

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct UsnRecordHeader {
    record_length: u32, // 0x00 4 bytes
    major_version: u16, // 0x04 2 bytes
    minor_version: u16, // 0x06 2 bytes
}

#[derive(Debug)]
struct ParsedUsnRecord {
    usn: i64,
    reason: u32,
    file_attributes: u32,
    file_reference_number: u64,
    parent_file_reference_number: u64,
    timestamp: i64, // or convert later
    name: String,
    major_version: u16,
}

// HELPER FUNCTIONS
/**
Takes in raw bytes of the USN record (for V2 records) and outputs a ParsedUsnRecord
*/
fn parse_usn_v2_record(record_bytes: &[u8]) -> Result<ParsedUsnRecord, String> {
    let raw: USN_RECORD_V2 =
        unsafe { std::ptr::read_unaligned(record_bytes.as_ptr() as *const USN_RECORD_V2) }; // raw bytes -> USN_RECORD

    let file_name_offset = raw.FileNameOffset as usize;
    let file_name_len = raw.FileNameLength as usize;
    let file_name_end = file_name_offset + file_name_len;

    if file_name_end > record_bytes.len() {
        return Err("filename out of bounds".into());
    }
    if file_name_len % 2 != 0 {
        return Err("Filename length is not valid UTF-16 length".into());
    }

    let file_name_u16 = unsafe {
        std::slice::from_raw_parts(
            record_bytes.as_ptr().add(file_name_offset) as *const u16,
            file_name_len / 2,
        )
    };

    let file_name = String::from_utf16_lossy(file_name_u16);

    Ok(ParsedUsnRecord {
        usn: raw.Usn,
        reason: raw.Reason,
        file_attributes: raw.FileAttributes,
        file_reference_number: raw.FileReferenceNumber,
        parent_file_reference_number: raw.ParentFileReferenceNumber,
        timestamp: raw.TimeStamp,
        name: file_name,
        major_version: raw.MajorVersion,
    })
}

/**
Takes in raw usn records and returns ParsedUsnRecord
 */
fn parse_one_record(bytes: &[u8]) -> Result<ParsedUsnRecord, String> {
    // parse header
    if bytes.len() < std::mem::size_of::<UsnRecordHeader>() {
        return Err("USN record too small for common header".to_string());
    }
    let header = unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const UsnRecordHeader) };
    if header.record_length as usize > bytes.len() {
        return Err("USN record length exceeds available bytes".to_string());
    };
    if header.record_length < std::mem::size_of::<UsnRecordHeader>() as u32 {
        return Err("USN record length is smaller than header".to_string());
    };

    match header.major_version {
        2 => parse_usn_v2_record(bytes),
        3 => parse_usn_v3_record(bytes),
        4 => parse_usn_v4_record(bytes),
        _ => Err(format!(
            "Unsupported USN record version {}",
            header.major_version
        )),
    }
}

fn parse_usn_v3_record(_bytes: &[u8]) -> Result<ParsedUsnRecord, String> {
    Err("USN_RECORD_V3 not implemented yet".to_string())
}

fn parse_usn_v4_record(_bytes: &[u8]) -> Result<ParsedUsnRecord, String> {
    Err("USN_RECORD_V4 not implemented yet".to_string())
}

const HANDLE_PATH: PCWSTR = windows::core::w!("\\\\.\\C:"); // \\.\C:

pub fn realtime_usn() -> Result<HANDLE, String> {
    println!("ENTERED realtime_usn()");
    // first we need the handle to the drive
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

    // now that we have the drive handle
    // we need to get the USN journal
    // we do this by calling DeviceIoControl and using the FSCTL_QUERY_USN_JOURNAL code
    // usn_journal_data contains info about the usn journal
    let buffer_size: u32 = 32 * 1024 * 1024;
    let mut usn_journal_data: USN_JOURNAL_DATA_V2 = unsafe { mem::zeroed() };
    let mut bytes_returned = 0u32;

    unsafe {
        DeviceIoControl(
            drive_handle,
            FSCTL_QUERY_USN_JOURNAL,
            None,
            0,
            Some(&mut usn_journal_data as *mut _ as *mut _),
            mem::size_of::<USN_JOURNAL_DATA_V2>() as u32,
            Some(&mut bytes_returned),
            None,
        )
    }
    .map_err(|e| format!("OS Error: {}", e))?;
    // first, we check to make sure the jounral hasnt wrapped
    // when the journal gets too large it wraps, meaning it discards old entries to make room for new ones (rescan needed)
    // we need to pass a starting usn which is where windows will start reading the usn entries from
    // we will be listening in realtime, and since we dont care about previous entries we use the journals
    // current NextUsn meaning we listen to every next change
    // for now since there isnt a lastusn in the database we will do this every time
    let last_saved_usn = usn_journal_data.NextUsn; // we start listening for changes from NOW on
    let debug_max_usn_iters = 100; // debug: max 100 times read usn will be called

    let mut input_configuration = READ_USN_JOURNAL_DATA_V0 {
        StartUsn: last_saved_usn,
        ReasonMask: 0xFFFF_FFFF,
        ReturnOnlyOnClose: 0,
        Timeout: 0,
        BytesToWaitFor: 1,
        UsnJournalID: usn_journal_data.UsnJournalID,
    };

    for _index in 0..debug_max_usn_iters {
        // read updates that have been made
        let mut journal_out_buffer = vec![0u8; 256 * 1024]; // 256 KB
        let mut returned_bytes = 0u32;

        unsafe {
            DeviceIoControl(
                drive_handle,
                FSCTL_READ_USN_JOURNAL,
                Some(&mut input_configuration as *mut _ as *mut c_void), // raw pointer to input buffer
                mem::size_of::<READ_USN_JOURNAL_DATA_V0>() as u32,
                Some(journal_out_buffer.as_mut_ptr() as *mut c_void),
                journal_out_buffer.len() as u32,
                Some(&mut returned_bytes),
                None,
            )
        }
        .map_err(|e| format!("OS Error: {}", e))?;
        // increment last usn if usn records were returned
        if returned_bytes >= mem::size_of::<i64>() as u32 {
            let next_usn = unsafe { *(journal_out_buffer.as_ptr() as *const i64) };

            input_configuration.StartUsn = next_usn;
        }

        // now we've recieved:
        // first 8 bytes = the new NextUSN
        // then packed usn records (USN_RECORD)
        // pass onto our helper to parse
        let mut offset = mem::size_of::<i64>();
        let bytes_used = returned_bytes as usize;
        if bytes_used < mem::size_of::<i64>() {
            continue;
        }

        while offset < bytes_used {
            let remaining = &journal_out_buffer[offset..bytes_used];

            let header =
                unsafe { std::ptr::read_unaligned(remaining.as_ptr() as *const UsnRecordHeader) };

            let record_len = header.record_length as usize;

            if record_len == 0 || record_len > remaining.len() {
                return Err("Invalid USN record length while parsing stream".to_string());
            }

            let record_bytes = &remaining[..record_len];

            let record = parse_one_record(record_bytes);
            println!("{:#?}", record);

            offset += record_len;
        }
    }

    Ok(drive_handle)
}
