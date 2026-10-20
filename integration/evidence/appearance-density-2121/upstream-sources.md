# Density boundary references

The official [GPUI Styled API](https://docs.rs/gpui/latest/gpui/trait.Styled.html) was opened to verify pixel-valued padding and gap behavior. The implementation reuses the existing pinned GPUI `.px`, `.py` and `.gap` consumers in `crates/ui/src/patterns/launcher.rs`; it adds no dependency or new framework API.

CyberManju's actual source `src/ui/tokens.ts` defines `DENSITY_SCALE` as compact unit 0.9 and comfortable unit 1.0. Native launcher semantic spacing uses exactly those unit factors. The separate browser font/control scales are deliberately NOT claimed or applied: native typography, icon sizes, explicit hit targets and compositor geometry remain unchanged.

The native runtime density is not a new persisted preference store. Existing profile schema/version, files, theme palettes and app preference owners remain unchanged. No public source or host UI test is evidence for NVIDIA initialization, Asterinas graphics, physical GPU acceleration or Linux parity.
