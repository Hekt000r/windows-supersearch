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
const NUM_ENTRIES_TO_SCAN: u32 = 10;

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
    pub content_offset: u16, // 0x14 offset to the content of the attribute
    pub indexed_flag: u8,    // 0x16
    pub padding: u8,         // 0x17
}

#[repr(C, packed)]

pub struct FileNameAttributeLayout {
    pub reference_to_parent: u64, // 0x00 file reference to the parent directory
    pub created_time: u64,        // 0x08
    pub altered_time: u64,        // 0x10
    pub mft_changed_time: u64,    // 0x18
    pub file_read_time: u64,      // 0x20
    pub file_size_allocated: u64, // 0x28
    pub file_size_real: u64,      // 0x30
    pub flags: u32,               // 0x38
    pub ea_and_reparse_attribs: u32, // 0x3c
    pub file_name_length: u8,     // 0x40
    pub file_name_namespace: u8, // 0x41, 1 = typical win32 names, 2 = msdos, 3 = some weird posix shit
    pub file_name_unicode: String, // 0x42, size is 2 bytes * L (length of filename)
}
// note: dont use filename attribute for stuff like file size because only updates when filename updates
// use $STANDARD_INFORMATION instead

fn read_filename_attribute(data: &[u8], content_offset: u32) {
    // we recieve data which is the entire attribute
    // we have content_offset which is where the content of the attribute (the actual filename UTf in this case) islocated
    // we need to grab the data at content offset
    // filename lengths vary so we need file_name_length at 0x40
    // get the offset which is where the filename attribute content actually starts
    let offset = content_offset as usize;
    let len_pos = offset + 0x40; //position of the filename length attribute
    let name_pos = offset + 0x42; // start of the bytes that have the actual utf16 chars for the filename

    let fn_length_chars: usize = data[len_pos] as usize;
    let fn_length_bytes: usize = fn_length_chars * 2; // length in chars -> length in bytes, 1 UTF char = 2 bytes
    println!("Filename length: {}", fn_length_chars);
    // now we can grab the filename itself which starts at 0x42 and ends at 0x42 + (2 bytes * L)
    let name_pos = offset + 0x42;
    let name_end = name_pos + fn_length_bytes;
    let fn_bytes = &data[name_pos..name_end];
    // turn the raw bytes into utf16 units (1 utf16 unit = 2 bytes)
    let utf16_units: Vec<u16> = fn_bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();

    // utf16 -> rust string
    let filename = String::from_utf16_lossy(&utf16_units);
    println!("Found filename: {}", filename)
}

fn get_filename_attribute(data: &[u8], start_offset: u32) {
    let mut current_offset: usize = start_offset as usize;

    while current_offset + std::mem::size_of::<AttributeHeader>() <= data.len() {
        let header: &AttributeHeader =
            unsafe { &*(data.as_ptr().add(current_offset) as *const AttributeHeader) };

        match header.type_id {
            0xFFFFFFFF => break,
            0x30 => {
                let body_offset = current_offset + header.content_offset as usize;
                println!("    Found $FILE_NAME at offset {}", current_offset);
                read_filename_attribute(data, body_offset as u32);
            }
            _ => (), // ignore other attribs
        }

        if header.length == 0 {
            break;
        }

        current_offset += header.length as usize;
    }
}

/* ==HELPERS== */
fn read_le_u64(bytes: &[u8]) -> u64 {
    let mut result = 0u64;
    for (i, &byte) in bytes.iter().enumerate() {
        result |= (byte as u64) << (i * 8);
    }
    result
}

fn read_le_i64(bytes: &[u8]) -> i64 {
    let unsigned = read_le_u64(bytes); // First, read as unsigned
    
    // Check if the highest bit of the highest byte is set (sign bit)
    let last_byte = bytes.last().unwrap_or(&0);
    if (last_byte & 0x80) != 0 {
        // Negative number: sign-extend it
        // Clear the sign bit and convert to negative
        let mask = (1u64 << (bytes.len() * 8)) - 1;
        -((!unsigned & mask) as i64 + 1)
    } else {
        // Positive number: just cast
        unsigned as i64
    }
}

// recieves runs list bytes, returns pairs of (lcn,length)
fn parse_data_runs(run_list: &[u8]) -> Vec<(u64,u64)> {
    // we recieve run_bytes which is a list of bytes of the entire dataruns list
    let mut runs = Vec::new();
    let mut current_lcn: u64 = 0u64; // current logical cluster number, the physical cluster on the disk
    let mut pos: usize = 0; // current byte position

    while pos < run_list.len() {
        let header_byte = run_list[pos]; // the header byte is the first byte of the entry (1 entry = 2 bytes)
        pos += 1;

        if header_byte == 0x00 { // 0x00 means end of list
            break;
        }

        // the header byte contains two values, split into 2 nibbles
        // high nibble contains the number of bytes used for the Offset field
        // low nibble contains the number of bytes used for the Length field
        // then there is the length field and offset field, whose size is contained in the header byte
        let length_bytes = (header_byte >> 4) as usize; // shift 4 bits so the last 4 bits are pushed off and u get the first 4 bits only
        let offset_bytes = (header_byte & 0x0F) as usize; // use a mask to grab the last nibble so last 4 bits

        // read the length, positive integer so unsigned
        let length = read_le_u64(&run_list[pos..pos + length_bytes]); // read le bytes from 
        // ^^ start of length attribute to end of it (pos+length_bytes)
        pos += length_bytes;

        let offset = read_le_i64(&run_list[pos..pos + offset_bytes]);
        pos += offset_bytes;

        // get the LCN for this run
        if offset > 0 {
            current_lcn += offset as u64
        } else {
            current_lcn -= offset.abs() as u64;
        }

        runs.push((current_lcn,length)); // add a data run start and length to the pair

    }

    runs 
}

/** takes the $MFT entry and returns data runs of the MFT */ 
fn parse_mft_data_attribute(data: &[u8],start:usize,end:usize) -> Vec<(u64,u64)> {
    // we are given data which is the entire MFT entry
    // we need to grab the DATA attribute which is at start, and finishes at END
    // data_bytes is the entire $DATA attribute
    let data_bytes: &[u8] = &data[start..end];
    
    // now cast the data attribute to a AttributeHeader
    let header = unsafe {
        &*(data_bytes.as_ptr() as *const AttributeHeader)
    };

    // assume resident since this is only for the MFT
    // run_list is the list of data runs
    // run list starts at content_offset bytes
    let run_list_start: usize = header.content_offset as usize;
    let run_list_end: usize = data_bytes.len(); // end of the attribute

    // grab the run list bytes
    // the run list is the actual content of the attribute, so a slice of the entire attribute
    let run_list_bytes = &data_bytes[run_list_start..run_list_end];

    // parse the data runs
    let data_runs = parse_data_runs(run_list_bytes);
    data_runs

}

// only for $MFT
fn get_data_attribute(data: &[u8], start_offset: u32) -> Option<usize> {
    let mut current_offset = start_offset as usize;
    while current_offset + size_of::<AttributeHeader>() <= data.len() {
        let header = unsafe { &*(data.as_ptr().add(current_offset) as *const AttributeHeader) };

        match header.type_id {
            0xFFFFFFFF => break, // End of attributes
            0x80 => {
                // Found the $DATA attribute!
                return Some(current_offset);
            }
            _ => {}
        }

        if header.length == 0 { break; }
        current_offset += header.length as usize;
    }
    None
}

fn parse_mft(data: &[u8]) -> Vec<(u64,u64)> {
    // attributes start at 0x30 in $MFT record
    const ATTRS_START: u32 = 0x30;

    // find $data attribute
    let attr_start = match get_data_attribute(data, ATTRS_START) {
        Some(start) => start,
        None => {
            eprintln!("ERROR: $DATA attribute not found in $MFT entry");
            return Vec::new();
        }
    };

    // read attribute header to get total length
    let header = unsafe {
        &*(data.as_ptr().add(attr_start) as *const AttributeHeader)
    };

    if header.non_resident != 1 {
        eprintln!("ERROR: $MFT $DATA attribute is resident (unexpected)");
        return Vec::new();
    }

    let attr_end = attr_start + header.length as usize;

    parse_mft_data_attribute(data, attr_start, attr_end)
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
    let mft_entry_size = volume_data.BytesPerFileRecordSegment;

    println!("MFT Location Found!");
    println!("- Start Cluster: {}", volume_data.MftStartLcn);
    println!("- Bytes per Cluster: {}", volume_data.BytesPerCluster);
    println!("- Physical Byte Offset: {}", mft_offset);
    println!("- MFT Entry Size: {}", mft_entry_size);

    println!("!=- Found MFT Location. Pointing drive handle to byte-location of MFT.");

    unsafe { SetFilePointerEx(drive_handle, mft_offset, None, FILE_CURRENT) }
        .map_err(|e| format!("SetFilePointerEx Failed: {}", e))?;

    println!(
        "Drive Handle pointing to start of MFT. Beginning read loop for {} entries.",
        NUM_ENTRIES_TO_SCAN
    );
    // old code
    for entry_index in 0..NUM_ENTRIES_TO_SCAN {
        let mut read_data_buffer: Vec<u8> = vec![0u8; mft_entry_size as usize];
        let mut bytes_read: u32 = 0;

        // Each ReadFile call advances the file pointer by however many bytes
        // were read, so as long as mft_entry_size is correct, the pointer
        // will already be positioned at the start of the next record after
        // each successful read — no extra seeking needed between iterations.
        let read_result = unsafe {
            ReadFile(
                drive_handle,
                Some(&mut read_data_buffer[..]),
                Some(&mut bytes_read as *mut u32),
                None,
            )
        };

        if let Err(e) = read_result {
            println!("ReadFile failed on entry {}: {}", entry_index, e);
            break;
        }

        if bytes_read < mft_entry_size {
            println!(
                "Entry {}: short read ({} of {} bytes) - stopping.",
                entry_index, bytes_read, mft_entry_size
            );
            break;
        }

        println!(
            "Entry {}: read {} bytes. Parsing attributes ...",
            entry_index, bytes_read
        );

        let header: &AttributeHeader =
            unsafe { &*(read_data_buffer.as_ptr() as *const AttributeHeader) };

        let content_offset = header.content_offset as u32;
        get_filename_attribute(&read_data_buffer, content_offset);
    }
    // new code
    // setup
    const CLUSTER_SIZE: usize = 4096; // bytes per disk cluster, usually 4kb
    const RECORD_SIZE: usize = 1024; // bytes in size of a MFT entry, usually 1kb
    const CHUNK_SIZE: usize = 1 * 1024 * 1024; // the chunk size which is how many bytes of the MFT at a time should be parsed
    
    // we get the data runs from our helpers
    // read the first MFT entry, which is $MFT
    let mut mft_entry: Vec<u8> = vec![0u8; mft_entry_size as usize];
    let mut bytes_read: u32 = 0;
    unsafe {
        ReadFile(
        drive_handle,
        Some(&mut mft_entry[..]),
        Some(&mut bytes_read as *mut u32),
        None,
    )
    }
    .map_err(|e| format!("ReadFile Failed: {}", e))?;
    let data_runs = parse_mft(&mft_entry);


    Ok(drive_handle)
}
