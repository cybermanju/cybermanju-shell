/* SPDX-License-Identifier: MPL-2.0 */
/* Ordinary C ET_REL fixture, NOT a Linux-kbuild module or loaded kernel code. */
extern int unresolved_kernel_symbol(int);
static int zero_initialized[32];
const char fixture_metadata[] __attribute__((section(".modinfo"))) =
    "description=ELF structural test only";
int fixture_entry(int value)
{
    zero_initialized[value & 31] = value;
    return unresolved_kernel_symbol(zero_initialized[value & 31]);
}
