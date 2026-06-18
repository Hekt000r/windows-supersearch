# Supersearch
NOTE: currently early in development, not usable yet but usable version is soon.

Windows Supersearch is a tool that lets you search for files on your entire computer instantly.
It has a beautiful UI designed to look and feel like MacOS's Spotlight.

Why? Tools like Everything by voidtools exists but it's UI is outdated and impractical, and not open-source.
Windows alternatives for Spotlight exist but they are all too slow.

How? While other Spotlight alternatives typically use FindFirstFile which is an API
provided by Microsoft to search files, it goes through several system layers,
has to take in account for permissions, security attributes, compression attributes, and a lot more.
Supersearch bypasses this by reading the MFT (Master File Table) which is essentially just a 
massive list of every file on disk and some of its metadata like the filename. Since Supersearch
just reads the raw bytes of the MFT, it doesnt have to deal with all the permission, security, and directory systems.

## Technical Details
This project uses Tauri. The scanner is written in Rust using nothing but win32 APIs via the win32 crate.
MFT entries are dumped into SQLite database.
The frontend UI is written with Solid + TailwindCSS
