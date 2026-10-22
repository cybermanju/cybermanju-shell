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
