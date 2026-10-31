# Install and try a binary release

Binary packaging starts with **v0.1.1**. **v0.1.0 is source-only**. Preview builds
on branches are Actions artifacts, not published releases. Download from the
[release page](https://github.com/KavishGajjar/jsonc-merge-rs/releases) after the
owner has published the tested draft.

| Platform | Target in the archive filename | Format |
| --- | --- | --- |
| Linux x64 | `x86_64-unknown-linux-musl` | `.tar.gz` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `.tar.gz` |
| macOS Intel | `x86_64-apple-darwin` | `.tar.gz` |
| Windows x64 | `x86_64-pc-windows-msvc` | `.zip` |

Linux uses static musl linking. Windows uses a static C runtime. The package
checks these properties and the executable architecture before upload. macOS
uses a 13.0 deployment target and system libraries; native smoke checks run on
GitHub's macOS 15 runners, not every older macOS version. These downloads are
not Developer ID/Authenticode signed or notarized. Operating-system download
prompts can therefore appear; source installation remains available.

## Verify your download

Download the platform archive and `SHA256SUMS` from the same release. Compare the
computed digest with the line for your archive:

macOS:

```sh
shasum -a 256 jsonc-merge-v0.1.1-aarch64-apple-darwin.tar.gz
```

Linux:

```sh
sha256sum jsonc-merge-v0.1.1-x86_64-unknown-linux-musl.tar.gz
```

Windows PowerShell:

```powershell
Get-FileHash .\jsonc-merge-v0.1.1-x86_64-pc-windows-msvc.zip -Algorithm SHA256
```

Checksums detect changed bytes; they are not a distributor signature. Archives
also contain a per-file manifest, build version/target/commit information,
documentation, the example and upstream license notices.

## Extract and run

For an Apple Silicon Mac (choose the matching filename from the table elsewhere):

```sh
tar -xzf jsonc-merge-v0.1.1-aarch64-apple-darwin.tar.gz
cd jsonc-merge-v0.1.1-aarch64-apple-darwin
./jsonc-merge --version
./jsonc-merge merge examples/launch/base.jsonc examples/launch/ours.jsonc examples/launch/theirs.jsonc --policy examples/launch/policy.json
```

Linux and Intel Mac archives use the same commands with their corresponding
filenames. For Windows PowerShell:

```powershell
Expand-Archive .\jsonc-merge-v0.1.1-x86_64-pc-windows-msvc.zip -DestinationPath .
cd .\jsonc-merge-v0.1.1-x86_64-pc-windows-msvc
.\jsonc-merge.exe --version
.\jsonc-merge.exe merge examples/launch/base.jsonc examples/launch/ours.jsonc examples/launch/theirs.jsonc --policy examples/launch/policy.json
```

The result keeps **Worker before API**, changes the API program to
**`dist/api.js`**, and keeps each comment with its record. The full expected text
is `examples/launch/expected.jsonc`. Rust, Python and Node.js are not runtime
requirements for the downloaded executable. Git is optional unless you use the
[Git merge driver](git-driver.md).

For source builds, use the [README instructions](../README.md#build-from-source).
