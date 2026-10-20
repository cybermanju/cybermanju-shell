/* Tiny freestanding Asterinas init for real NVMe + VirtIO block I/O validation.
 * x86-64 Linux syscall ABI; no libc or runtime is linked.
 */
typedef unsigned long usize;
typedef long ssize;

static inline long syscall3(long nr, long a1, long a2, long a3) {
    long ret;
    __asm__ volatile ("syscall" : "=a"(ret) : "a"(nr), "D"(a1), "S"(a2), "d"(a3) : "rcx", "r11", "memory");
    return ret;
}
static inline long syscall5(long nr, long a1, long a2, long a3, long a4, long a5) {
    register long r10 __asm__("r10") = a4;
    register long r8 __asm__("r8") = a5;
    long ret;
    __asm__ volatile ("syscall" : "=a"(ret) : "a"(nr), "D"(a1), "S"(a2), "d"(a3), "r"(r10), "r"(r8) : "rcx", "r11", "memory");
    return ret;
}

static long write_all(int fd, const void *buf, usize len) {
    const char *p = (const char *)buf;
    usize done = 0;
    while (done < len) {
        long n = syscall3(1, fd, (long)(p + done), (long)(len - done));
        if (n <= 0) return n ? n : -1;
        done += (usize)n;
    }
    return (long)done;
}
static void say(const char *s, usize n) { (void)write_all(1, s, n); }
#define SAY(s) say((s), sizeof(s) - 1)

static void fail(const char *reason, usize len) {
    SAY("CONSUMER_IO FAIL: ");
    say(reason, len);
    SAY("\n");
    (void)syscall3(60, 1, 0, 0); /* exit(1); runner rejects absence of PASS. */
    for (;;) __asm__ volatile ("pause");
}
#define FAIL(s) fail((s), sizeof(s) - 1)

static int io_full(long fd, void *buf, usize len, int writing) {
    usize done = 0;
    while (done < len) {
        long n = syscall3(writing ? 1 : 0, fd, (long)((char *)buf + done), (long)(len - done));
        if (n <= 0) return 0;
        done += (usize)n;
    }
    return 1;
}
static unsigned char initial_byte(unsigned int seed, usize i) {
    return (unsigned char)(i * 37u + seed);
}
static unsigned char scratch_byte(unsigned int seed, usize i) {
    return (unsigned char)(i * 17u + (seed ^ 0xa5u));
}

static void verify_device(const char *path, unsigned int seed,
                          const char *label, usize label_len) {
    unsigned char actual[4096];
    unsigned char expected[4096];
    long fd = syscall3(2, (long)path, 2 /* O_RDWR */, 0);
    if (fd < 0) FAIL("open block device failed");
    if (!io_full(fd, actual, sizeof(actual), 0)) FAIL("short/error reading fixture bytes");
    for (usize i = 0; i < sizeof(expected); ++i) expected[i] = initial_byte(seed, i);
    for (usize i = 0; i < sizeof(expected); ++i) {
        if (actual[i] != expected[i]) FAIL("fixture data mismatch (wrong device, offset, or I/O)");
    }

    /* Writable fixture disks: exercise both directions at a separate 4 KiB area. */
    for (usize i = 0; i < sizeof(expected); ++i) expected[i] = scratch_byte(seed, i);
    if (syscall3(8, fd, 4096, 0) != 4096) FAIL("seek to scratch area failed");
    if (!io_full(fd, expected, sizeof(expected), 1)) FAIL("scratch write failed");
    if (syscall3(74, fd, 0, 0) < 0) FAIL("scratch fsync failed");
    if (syscall3(8, fd, 4096, 0) != 4096) FAIL("seek back to scratch area failed");
    for (usize i = 0; i < sizeof(actual); ++i) actual[i] = 0;
    if (!io_full(fd, actual, sizeof(actual), 0)) FAIL("scratch readback failed");
    for (usize i = 0; i < sizeof(actual); ++i) {
        if (actual[i] != scratch_byte(seed, i)) FAIL("scratch readback mismatch");
    }
    (void)syscall3(3, fd, 0, 0);
    static const char prefix[] = "CONSUMER_IO DEVICE PASS: ";
    static const char suffix[] = " verified 4096 fixture bytes + 4096-byte scratch write/read\n";
    char message[192];
    usize at = 0;
    if ((sizeof(prefix) - 1) + label_len + (sizeof(suffix) - 1) > sizeof(message))
        FAIL("device marker too long");
    for (usize i = 0; i < sizeof(prefix) - 1; ++i) message[at++] = prefix[i];
    for (usize i = 0; i < label_len; ++i) message[at++] = label[i];
    for (usize i = 0; i < sizeof(suffix) - 1; ++i) message[at++] = suffix[i];
    say(message, at);
}

__attribute__((force_align_arg_pointer)) void _start(void) {
    /* Asterinas deliberately does not auto-mount devtmpfs for rdinit initramfs. */
    static const char src[] = "devtmpfs";
    static const char target[] = "/dev";
    static const char fstype[] = "devtmpfs";
    if (syscall5(165, (long)src, (long)target, (long)fstype, 0, 0) < 0)
        FAIL("mount devtmpfs on /dev failed");

    verify_device("/dev/nvme0n1", 0x23u, "nvme0n1 (NVMe)", sizeof("nvme0n1 (NVMe)") - 1);
    verify_device("/dev/vda", 0x7bu, "vda (VirtIO modern)", sizeof("vda (VirtIO modern)") - 1);
    verify_device("/dev/vdb", 0xd1u, "vdb (VirtIO legacy)", sizeof("vdb (VirtIO legacy)") - 1);
    SAY("ASTERINAS_CONSUMER_IO_PASS: NVMe + VirtIO modern + VirtIO legacy real block I/O verified\n");

    /* Asterinas maps poweroff to QEMU's isa-debug-exit device (exit status 33). */
    (void)syscall5(169, 0xfee1dead, 0x28121969, 0x4321fedc, 0, 0);
    FAIL("successful poweroff syscall unexpectedly returned");
}
