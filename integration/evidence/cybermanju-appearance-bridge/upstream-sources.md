# Official references checked for this pass

[Asterinas 0.19.0 announcement](https://asterinas.github.io/2026/10/13/announcing-asterinas-0.19.0.html), published 2026-10-13: Asterinas NixOS runs Xorg modesetting with Mesa software OpenGL. The new DRM subsystem provides /dev/dri, GEM dumb buffers CPU-mappable by userspace, and KMS objects. This does not establish native NVIDIA/AMD acceleration, Vulkan/Wayland/GPUI/Tauri compatibility or physical-GPU performance in the local fork.

[url 2.5.8 API](https://docs.rs/url/2.5.8/url/struct.Url.html): Url::parse parses an absolute URL; scheme() is lowercased, host_str() is optional, username() may be empty, password() is independently optional (including an empty username with a password). Native wallpaper metadata validation checks all four and never fetches the URL. Version 2.5.8 was already present in the GPUI Cargo.lock before adding it as a direct dependency.
