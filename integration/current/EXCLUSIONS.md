# What is excluded, and why

`exclusions.json` is the exact path inventory. Directory entries exclude their full descendant subtree; file entries exclude only that named file. Source paths identify the original worktree, not a public download location.

- Nested `.git/`: repository internals, remotes, credentials and full upstream history are not copied. The target's own `.git/`, remote and initial commit are preserved.
- Every discovered `target/`, `node_modules/`, `__pycache__/`, `.cache/`, `.direnv/` and similar temporary cache: generated builds, installed dependencies or bytecode. Source directories merely named `build` are **not** blanket-excluded; for example Asterinas OSDK's source command module is retained.
- Generated `.iso`, `.raw`, `.cpio`, firmware `.bin`, compiled objects/libraries/executables, SPIR-V outputs and TypeScript `.tsbuildinfo`: rebuildable or external binary artifacts, not requested source. The app's `tsconfig.node.tsbuildinfo` is excluded even though present in its worktree.
- Temporary scratch files: including the exact temporary file under `local-prs/` recorded in the inventory.
- Credentials and local secret-bearing configuration: no tokens, private keys, credential stores or private environment files are included. Original `.envrc` source definitions are retained and scanned. Synthetic redaction-test fixtures and private-key delimiter strings are not operational credentials; nine scanner false positives are individually reviewed in `secret-scan.json`.
- Evidence binary screenshots, test executables, generated image captures and an external specification PDF: omitted in favor of text results. Original app/GPUI icons, illustrations and documentation GIFs remain source assets; the current manifest lists these explicitly.
- Evidence `before/` and `source-snapshot/` duplicate historical trees, rootfs and build subtrees: omitted to avoid redundant sources, installed filesystem payloads and firmware. Current source, canonical patches and adjacent text reports remain included. Historical notes may refer to omitted paths; those are not promised as runnable dependencies.
- Text evidence over 512 KiB: saved as `*.excerpt.txt` containing first 8 KiB and last 32 KiB, with the original full length and SHA-256. The original path is recorded as `file-excerpted`, never silently represented as a complete log.
- External `vendor-drivers/`, `installed/`, `toolchain/`, and the separate `asterinas-irq-retire-local/` verification worktree: outside the requested three current source trees. NVIDIA installer/firmware/built outputs are not vendored. Their requirements and prior compilation results remain documented.

No source worktree was deleted or cleaned. There were no source-copy failures. Licensing/copyright/NOTICE files are preserved wherever present. The complete list, including actual matched paths and reasons, is in `exclusions.json`; the forbidden-name rules do not imply that every listed artifact type existed.

## Existing target-local cache

Nine pre-existing ignored `asterinas/tools/graphics/__pycache__/*.pyc` files remain on disk, untouched and untracked, per explicit user instruction. They are not included source/evidence manifest entries or commit content. Exact paths, byte counts and hashes are in `retained-local-cache.json`; the source exclusion list is supplemented by this target-local exception.
