# Build and publish a binary preview

`v0.1.0` remains the original source-only release. The first binary release uses
the new **v0.1.1** version. Do not move the existing v0.1.0 tag.

## What the workflow does

The `binary-release` workflow runs on pushes to main, pull requests, manual
dispatches and `v*` tag pushes. Branch/PR/manual runs only produce downloadable
Actions artifacts. They never create or publish a release.

Four native jobs build and test Linux x64 musl, Apple Silicon macOS, Intel macOS
and Windows x64 MSVC. Each job runs release-profile Rust tests offline after
fetching the locked dependencies, inspects architecture/runtime dependencies,
packages the executable with documentation/examples/notices, and executes the
extracted package from a fresh temporary directory. A target that fails any
step produces no successful package artifact. The Rust code and bundled demo use
the same source commit and workspace version.

The Linux build uses Rust's bundled musl runtime and default system linker.
No dependency compiles native C code. Avoid overriding the linker with a distro
`musl-gcc` wrapper: some wrappers add a dynamic interpreter to static PIEs. The
ELF dependency inspection and actual executable checks both guard this boundary.

An assembly job requires all four archives and verifies their embedded manifests,
commit/version metadata and checksum sidecars. It creates `SHA256SUMS`. On an
exact matching version-tag push only, a final job has repository write access
to create an **unpublished draft pre-release** with four archives and the combined
checksums. The draft job checks the remote tag's commit. It cannot run on a PR or
manual preview. No personal access token or custom secret is required; it uses
the job's `GITHUB_TOKEN`.

Existing published releases are never overwritten. An identical retry can reuse
matching draft assets or fill missing ones. A differing existing asset fails
explicitly instead of replacing it. Inspect that draft and, if appropriate,
delete the incomplete draft (not its tag) before rerunning the failed jobs.
Leave the draft unpublished until the workflow has completely finished.

## Exact owner steps for v0.1.1

The local changes are prepared but not committed or pushed automatically.

1. From the project directory, review and commit:

   ```sh
   git --no-pager diff --stat
   git add .github/workflows/ci.yml .github/workflows/binaries.yml .gitignore .dockerignore Cargo.toml Cargo.lock Dockerfile README.md CHANGELOG.md CONTRIBUTING.md docs scripts examples/launch
   git commit -m "Add tested binary releases and a checked README demo"
   git push origin main
   ```

2. In GitHub → **Actions**, wait for both **verify** and **binary-release** on
   this same commit to succeed. The **draft** job is intentionally skipped on
   main. Preview artifacts are available from the binary-release run summary.
   This first hosted run is required to validate the native Mac/Windows packages;
   local Linux Docker checks cannot execute those operating systems' binaries.

3. Once the preview is green, tag the exact tested commit. Ensure no newer local
   edits or commits were added in between:

   ```sh
   git status --short
   git tag -a v0.1.1 -m "JSONC Merge v0.1.1 binary preview"
   git push origin v0.1.1
   ```

   The tag must equal `v` plus the workspace version. Incorrect tags fail without
   uploading releases. If v0.1.1 already exists, do not force-move it; inspect the
   existing release/run first.

4. Wait for both tag workflows to pass. Open **Releases** and find the generated
   **v0.1.1 draft**. Confirm it has four platform archives and `SHA256SUMS`.
   Notes come from `docs/releases/v0.1.1.md`.

5. Review the draft, keep **pre-release** selected and **latest** unselected, then
   click **Publish release**. No manual binary attachment is needed.

For later versions, update the workspace version, its two Cargo.lock package
entries, release notes and version-specific installation examples together.

## Local checks

Packaging/publisher unit tests use synthetic executable headers and a fake GitHub
transport. They never upload real releases:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts/tests -v
```

The regular offline Docker verification also runs those tests. For an actual
Linux x64 musl package, including on an Apple Silicon development machine:

```sh
docker build --platform linux/amd64 --target release-check -t jsonc-merge-release-check .
docker run --rm --platform linux/amd64 --network none --read-only \
  --tmpfs /tmp:rw,exec,size=2g,mode=1777 jsonc-merge-release-check
```

Image setup downloads dependencies and the target standard library. Test,
packaging and extracted-binary execution run without networking. Mount a separate
writable evidence directory and set `EVIDENCE_DIR` if you want to retain the
archive, checksum sidecar and before/after source manifests. Local packages have
`source_revision: null`; GitHub packages record the checked-out commit. Archive
metadata is stable for identical payloads, but reproducible compiler output and
distributor signing are not claimed.
