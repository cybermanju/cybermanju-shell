// SPDX-License-Identifier: MPL-2.0
fn main() {
    let path = std::env::args().nth(1).expect("expected ELF object path");
    let bytes = std::fs::read(path).expect("read fixture");
    let result = linux_module_elf::inspect(&bytes).expect("inspect real C ET_REL fixture");
    assert!(result.section_count > 2);
    println!(
        "C ET_REL structural inspection passed: {} sections; NOT loaded or ABI-validated",
        result.section_count
    );
}
