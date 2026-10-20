# Filesystem boundary references

The safe native-loader change uses the existing Unix `OpenOptionsExt::custom_flags` API and already-locked libc 0.2.187 constants; no unsafe Rust or FFI calls were added.

- [Official Rust OpenOptionsExt](https://doc.rust-lang.org/std/os/unix/fs/trait.OpenOptionsExt.html): custom flags augment the access mode established by `.read(true)`; they do not replace it. Read directly during this execution.
- [Linux open(2)](https://man7.org/linux/man-pages/man2/open.2.html): O_NOFOLLOW rejects a symlink in the terminal pathname component but still follows parent-component symlinks. O_NONBLOCK avoids FIFO-open waiting; it does not impose a latency bound on regular-file or block-device I/O. O_NOCTTY prevents acquiring a controlling terminal. Read directly during this execution.

Target source mapping, not runtime certification:

- `asterinas/kernel/core/src/fs/file/file_attr/open_args.rs`: from_flags_and_mode separates creation/status flags; follow_tail_link checks O_NOFOLLOW.
- `asterinas/kernel/core/src/fs/file/file_attr/creation_flags.rs`: O_NOFOLLOW, O_NOCTTY and O_CLOEXEC definitions.
- `asterinas/kernel/core/src/fs/pipe/common.rs`: read-only named-pipe admission waits for a writer only if O_NONBLOCK is absent (lines 241–251 in the inspected source).

The actual CLI/filesystem regressions run on the Linux host. This source audit does not establish GPUI, libc/loader compatibility or these calls working end-to-end under Asterinas. Regular-file contents can still be concurrently modified; this is neither an immutable snapshot nor a complete filesystem sandbox.
