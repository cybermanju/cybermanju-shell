// SPDX-License-Identifier: MPL-2.0
#if !defined(__x86_64__)
#error "memcmp C probe is restricted to x86_64"
#endif

typedef __SIZE_TYPE__ size_t;
typedef unsigned char uint8_t;
typedef unsigned int uint32_t;
typedef int (*memcmp_fn)(const void *, const void *, size_t);
typedef struct { uint32_t calls, passed_mask, buffers_preserved; } CReport;
typedef struct { uint8_t pre, data[260], post; } Buf;
_Static_assert(sizeof(int) == 4, "int must be 32 bits");
_Static_assert(sizeof(void *) == 8, "pointer must be 64 bits");
_Static_assert(sizeof(size_t) == 8 && (size_t)-1 > 0, "size_t must be unsigned 64 bits");
_Static_assert(sizeof(CReport) == 12 && _Alignof(CReport) == 4, "report layout");

static int oracle(const uint8_t *x, const uint8_t *y, size_t n) {
    for (size_t i = 0; i < n; ++i) {
        uint8_t l = x[i], r = y[i];
        if (l != r) return (int)l - (int)r;
    }
    return 0;
}

/* The callback cannot choose its own expected result. Snapshots are per call,
 * so deliberate changes between cases cannot conceal a callback write. */
#define RUN(bit, x, y, n) do { \
    for (size_t j = 0; j < 8; ++j) saved[j] = *actual[j]; \
    int expected = oracle((const uint8_t *)(x), (const uint8_t *)(y), (n)); \
    int got = callback((x), (y), (n)); \
    ++report->calls; \
    if (got == expected) report->passed_mask |= (uint32_t)1u << (bit); \
    for (size_t j = 0; j < 8; ++j) { \
        const uint8_t *now = (const uint8_t *)actual[j]; \
        const uint8_t *before = (const uint8_t *)&saved[j]; \
        for (size_t k = 0; k < sizeof(Buf); ++k) \
            if (now[k] != before[k]) report->buffers_preserved = 0; \
    } \
} while (0)

void cybermanju_memcmp_c_probe(memcmp_fn callback, CReport *report) {
    /* Call-local buffers make the trusted wrapper reentrant. No shared C data. */
    Buf a = { .pre = 0xA5, .post = 0x5A }, b = { .pre = 0x3C, .post = 0xC3 };
    Buf c = { .pre = 0x69, .post = 0x96 }, d = { .pre = 0x87, .post = 0x78 };
    Buf e = { .pre = 0x12, .post = 0x21 }, f = { .pre = 0xAB, .post = 0xBA };
    Buf g = { .pre = 0xCD, .post = 0xDC }, h = { .pre = 0xEF, .post = 0xFE };
    Buf saved[8];
    const Buf *actual[8] = { &a, &b, &c, &d, &e, &f, &g, &h };
    report->calls = 0;
    report->passed_mask = 0;
    report->buffers_preserved = 1;
    for (size_t i = 0; i < 260; ++i) {
        a.data[i] = (uint8_t)i; b.data[i] = (uint8_t)i;
        c.data[i] = (uint8_t)(i ^ 0x55u); d.data[i] = (uint8_t)(i ^ 0x55u);
        e.data[i] = (uint8_t)(i + 17u); f.data[i] = (uint8_t)(i + 17u);
        g.data[i] = (uint8_t)(i * 3u); h.data[i] = (uint8_t)(i * 3u);
    }
    a.data[3] = 9; b.data[3] = 10;
    a.data[7] = 0; b.data[7] = 1;
    c.data[2] = 0; d.data[2] = 0; c.data[3] = 0x80; d.data[3] = 0xff;
    e.data[0] = 0xff; f.data[0] = 0x00;

    RUN(0, g.data, h.data, 8);                    /* equal */
    RUN(1, a.data, b.data, 1);                    /* equal prefix */
    RUN(2, a.data, b.data, 4);                    /* early difference */
    b.data[3] = a.data[3];
    RUN(3, a.data, b.data, 8);                    /* last-byte difference */
    b.data[3] = 10;
    RUN(4, c.data, d.data, 8);                    /* embedded NUL */
    RUN(5, c.data+3, d.data+3, 1);                /* unsigned 0x80/ff */
    RUN(6, e.data, f.data, 1);                    /* reverse unsigned */
    RUN(7, a.data+1, b.data+2, 5);                /* unaligned */
    RUN(8, a.data, a.data+1, 16);                 /* stable overlap */
    RUN(9, g.data, g.data, 16);                   /* identical pointer */
    RUN(10, (const void *)0, (const void *)0, 0); /* explicit zero/null contract */
    RUN(11, g.data, h.data, 257);                 /* full long equal read */
    RUN(12, a.data+1, b.data+1, 8);
    RUN(13, c.data+255, d.data+255, 5);           /* final data byte, not guard */
    RUN(14, e.data+5, f.data+5, 32);
    h.data[256] ^= 1;
    RUN(15, g.data, h.data, 257);                /* difference only at the end */
}

typedef void *(*memcpy_fn)(void *, const void *, size_t);
typedef struct {
    uint32_t calls, passed_mask, source_preserved, guards_preserved;
    uint32_t returns_destination, destination_verified;
} MemcpyCReport;
_Static_assert(sizeof(MemcpyCReport) == 24 && _Alignof(MemcpyCReport) == 4,
               "memcpy report layout");

typedef struct { uint8_t pre, data[260], post; } CopyBuf;
_Static_assert(__builtin_offsetof(CopyBuf, data) == 1 && sizeof(CopyBuf) == 262,
               "copy oracle byte layout");
static void init_copy(CopyBuf *src, CopyBuf *dst, unsigned seed) {
    src->pre = 0xA5; src->post = 0x5A;
    dst->pre = 0x3C; dst->post = 0xC3;
    for (size_t i = 0; i < 260; ++i) {
        src->data[i] = (uint8_t)(i * 37u + seed);
        dst->data[i] = (uint8_t)(0xE1u - i - seed);
    }
}

/* Each call has private source/destination/snapshots; oracle precedes callback.
 * Every positive-length pair is disjoint, including deliberately unaligned spans. */
#define COPY_CASE(bit, length, so, doff, nullsrc, nulldst) do { \
    CopyBuf srcbuf, dstbuf; init_copy(&srcbuf, &dstbuf, (bit) * 11u); \
    uint8_t src_before[sizeof(srcbuf)], dst_before[sizeof(dstbuf)]; \
    uint8_t expected[sizeof(dstbuf)]; \
    for (size_t q = 0; q < sizeof(srcbuf); ++q) src_before[q] = ((uint8_t *)&srcbuf)[q]; \
    for (size_t q = 0; q < sizeof(dstbuf); ++q) { dst_before[q] = ((uint8_t *)&dstbuf)[q]; expected[q] = dst_before[q]; } \
    size_t n = (length); size_t sx = (so), dx = (doff); \
    if (n) for (size_t q = 0; q < n; ++q) expected[1 + dx + q] = srcbuf.data[sx + q]; \
    void *dest = (nulldst) ? (void *)0 : (void *)(dstbuf.data + dx); \
    const void *source = (nullsrc) ? (const void *)0 : (const void *)(srcbuf.data + sx); \
    void *got = callback(dest, source, n); ++report->calls; \
    int source_ok = 1, guards_ok = srcbuf.pre == src_before[0] && srcbuf.post == src_before[sizeof(srcbuf)-1] && dstbuf.pre == dst_before[0] && dstbuf.post == dst_before[sizeof(dstbuf)-1]; \
    for (size_t q = 0; q < sizeof(srcbuf); ++q) if (((uint8_t *)&srcbuf)[q] != src_before[q]) source_ok = 0; \
    int dest_ok = 1; for (size_t q = 0; q < sizeof(dstbuf); ++q) if (((uint8_t *)&dstbuf)[q] != expected[q]) dest_ok = 0; \
    int ret_ok = got == dest; \
    if (!source_ok) report->source_preserved = 0; \
    if (!guards_ok) report->guards_preserved = 0; \
    if (!ret_ok) report->returns_destination = 0; \
    if (!dest_ok) report->destination_verified = 0; \
    if (source_ok && guards_ok && ret_ok && dest_ok) report->passed_mask |= (uint32_t)1u << (bit); \
} while (0)

void cybermanju_memcpy_c_probe(memcpy_fn callback, MemcpyCReport *report) {
    report->calls = report->passed_mask = 0;
    report->source_preserved = report->guards_preserved = 1;
    report->returns_destination = report->destination_verified = 1;
    COPY_CASE(0, 0, 0, 0, 1, 1);                 /* null/null zero */
    COPY_CASE(1, 0, 0, 0, 1, 0);                 /* null source, valid destination */
    COPY_CASE(2, 0, 0, 0, 0, 1);                 /* valid source, null destination */
    COPY_CASE(3, 1, 0, 0, 0, 0);                  /* single byte */
    COPY_CASE(4, 1, 1, 3, 0, 0);                  /* unaligned byte */
    COPY_CASE(5, 2, 2, 5, 0, 0);
    COPY_CASE(6, 7, 3, 1, 0, 0);
    COPY_CASE(7, 8, 8, 9, 0, 0);
    COPY_CASE(8, 15, 4, 17, 0, 0);
    COPY_CASE(9, 16, 16, 3, 0, 0);
    COPY_CASE(10, 31, 7, 5, 0, 0);
    COPY_CASE(11, 32, 32, 11, 0, 0);
    COPY_CASE(12, 255, 2, 1, 0, 0);
    COPY_CASE(13, 257, 1, 2, 0, 0);
    COPY_CASE(14, 260, 0, 0, 0, 0);              /* full boundaries */
    COPY_CASE(15, 257, 2, 3, 0, 0);              /* final copied byte */
}
