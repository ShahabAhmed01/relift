# Quick Start

## Scan a Disk Image

1. Image a raw device (recommended over scanning a mounted volume):

```sh
relift image \\.\PhysicalDrive1 --dest D:\images\evidence.dd
```

2. Scan the image:

```sh
relift scan D:\images\evidence.dd --session D:\session --live --carve
```

3. Recover deleted files to a separate destination:

```sh
relift recover D:\session --dest D:\recovered
```

## Continue an Interrupted Scan

If a scan was interrupted, resume it (the source must be unchanged):

```sh
relift scan image.dd --session ./session --resume
```

## List Devices

```sh
relift devices
```

## Run Self-Tests

```sh
relift doctor
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | OK |
| 1 | Runtime error |
| 2 | Usage error |
| 3 | Unsafe destination (same physical device) |
| 4 | Source mismatch (session vs actual source) |
| 5 | Partial recovery |

See [`docs/CLI.md`](CLI.md) for the full command reference.
