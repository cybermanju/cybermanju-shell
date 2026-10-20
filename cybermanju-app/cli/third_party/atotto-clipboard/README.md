# Local `github.com/atotto/clipboard` fork

Based on upstream `github.com/atotto/clipboard` v0.1.4; original BSD license is preserved in [LICENSE](LICENSE). The non-Android platform implementations are retained unchanged apart from repository-required file headers.

On Android, package initialization no longer calls `exec.LookPath`: Go's Android path uses `faccessat2`, which Termux's seccomp policy blocks with `SIGSYS`. Clipboard commands are invoked lazily by absolute `$PREFIX/bin/termux-clipboard-{get,set}` paths, so normal CLI startup and `cyb setup` do not require Termux:API. Clipboard read/write remains optional and fails gracefully when the Termux:API package is absent.
