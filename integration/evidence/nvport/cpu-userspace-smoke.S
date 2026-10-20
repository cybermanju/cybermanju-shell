/* Local CPU-only boot probe. Not a GPU benchmark. */
.section .rodata
message:
    .ascii "ASTERINAS_CPU_USERSPACE_SMOKE passed (not a GPU test)\n"
.set message_len, .-message
.section .text
.global _start
_start:
    mov $1, %eax
    mov $1, %edi
    lea message(%rip), %rsi
    mov $message_len, %edx
    syscall
    cmp $message_len, %rax
    jne failure
    mov $169, %eax
    mov $0xfee1dead, %edi
    mov $0x28121969, %esi
    mov $0x4321fedc, %edx
    xor %r10d, %r10d
    syscall
failure:
    mov $60, %eax
    mov $1, %edi
    syscall
    jmp failure
.section .note.GNU-stack,"",@progbits
