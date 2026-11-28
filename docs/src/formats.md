# Carved Format Reference

Relift searches for registered signatures and emits a candidate only when the
corresponding validator accepts the structure. A candidate passing validation
is evidence about its container structure; it is not proof that the file is
complete, uncorrupted, or openable in every application.

The implementation is in
[`crates/relift-core/src/carve.rs`](https://github.com/ShahabAhmed01/relift/blob/main/crates/relift-core/src/carve.rs).
The 153 formats below are registered for carving; a format without a working
validator is not emitted.

## Registered formats

| Group | Formats |
|---|---|
| Images | JPEG, PNG, GIF, BMP, TIFF, WebP, HEIC, AVIF, TGA, PCX, PBM, PGM, PPM, ICO, ICNS, CUR, EMF, WMF, PSD, XCF, CDR, AI, EPS, PS, EXR, HDR, ORF, RW2, X3F, CRW, CR3, RAF, MRW, DDS |
| Documents | PDF, DOCX, XLSX, PPTX, DOC, XLS, PPT, PUB, WPD, WPS, XLR, ODP, ODS, ODT, RTF, EPUB, TEX, LYX, FB2, PAGES, NUMBERS, KEY |
| Archives and containers | ZIP, 7z, RAR4, RAR5, gzip, bzip2, xz, zstd, TAR, ISO 9660, CAB, ACE, ARJ, LZH, LZO, PAR2, RPM, XAR, DEB, BKF, ARC, ZOO, WIM, APK |
| Video | MP4/ISO-BMFF, MKV, MPEG-TS, FLV, SWF, MXF, DPX, VOB, ASF, AVI |
| Audio | MP3, WAV/RIFF, Ogg, FLAC, AMR, AAC, M4A, WMA, AIFF, AU, MID, IT, XM, S3M, MOD, APE, VOC, SPX, WV, TTA |
| Databases | SQLite, MDB, ACCDB, IBd |
| System files | PST/OST, LNK, EVTX, DBX, MSG, MBX, EML, NSF |
| Executables | PE (EXE/DLL/SYS), ELF, Mach-O, Java Class, DEX, COM |
| Fonts | TTF, OTF, WOFF, WOFF2, PFB |
| Disk / forensic | E01 (EnCase), GHO, DAR |
| Apple | Plist (binary and XML), AppleSingle |
| Game / ROM | NES |

The MP4/ISO-BMFF validator does not separately classify MOV. WebM is handled by
the MKV/Matroska validator. DOCX/XLSX/PPTX are checked as ZIP containers with
expected package entries. Relift does not extract archive members.

## Not emitted

- Shn (Shorten), NSF (Lotus Notes), EML, VFD, ROM, N64, GBA, SFC, BIN have no
  working validator in this build and are not emitted despite having markers.
- ZIP64: shares the `PK\x03\x04` local-header magic with ZIP and is handled by
  the existing ZIP validator. A dedicated ZIP64 end-of-central-directory
  validator is not implemented.

See [`support-matrix.md`](support-matrix.md) and
[`GAP_REPORT.md`](GAP_REPORT.md) for the current support boundary.

## Validation limits

Validators enforce format-specific length, structure, checksum, and/or end
marker rules where implemented. Validation depth differs by format and is not
a substitute for a format library or application-level verification. Embedded
compressed or encrypted content is recovered as bytes; Relift does not decrypt
or extract it.

The confidence labels are evidence categories, not probabilities. A validated
fragment may still be incomplete because the source file was fragmented,
overwritten, or truncated. Recovery SHA-256 verification checks that the bytes
written to the destination match the bytes read back; it does not prove they
match an original copy.

## Adding a format

Follow the contribution guide and add valid, truncated, and malformed test
fixtures before registering a new marker. Do not emit a format based only on a
magic-byte match. The `registered_format_count_is_stable` unit test fails when
the marker set changes, which is the prompt to update this page and the support
matrix.



