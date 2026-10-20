// SPDX-License-Identifier: MPL-2.0
/* Host-only tests: cc this with diagnostics_format.c and NVIDIA include roots. */
#include <nvtypes.h>
#include <os-interface.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

int aster_nv_snprintf(char *buffer, NvU32 capacity, const char *fmt, ...);
int nv_printf(NvU32 level, const char *fmt, ...);

static char logged[2048];
static NvU32 logged_length;

void aster_nv_log_bytes(const char *bytes, NvU32 length)
{
    logged_length = length < sizeof(logged) ? length : (NvU32)sizeof(logged);
    memcpy(logged, bytes, logged_length);
    (void)fwrite(bytes, 1, length, stderr);
    (void)fputc('\n', stderr);
}

static int failures;
#define CHECK(condition) do { \
    if (!(condition)) { \
        fprintf(stderr, "FAIL %s:%d: %s\n", __FILE__, __LINE__, #condition); \
        ++failures; \
    } \
} while (0)

#define COMPARE(format, ...) do { \
    char expected[512], actual[512]; \
    int er = snprintf(expected, sizeof(expected), format, ##__VA_ARGS__); \
    int ar = aster_nv_snprintf(actual, sizeof(actual), format, ##__VA_ARGS__); \
    if (er != ar || (er >= 0 && strcmp(expected, actual) != 0)) { \
        fprintf(stderr, "format mismatch [%s]: libc=%d <%s>, ours=%d <%s>\n", \
                format, er, expected, ar, actual); \
        ++failures; \
    } \
} while (0)

int main(void)
{
    char a[64], b[64], big[1400], exact[1024];
    int n = 77;
    int r;

    COMPARE("signed=%d unsigned=%u", -123, 4000000000U);
    COMPARE("oct=%#o hex=%#x HEX=%#X", 0755U, 0xfeedU, 0xfeedU);
    COMPARE("sign=[%+08d][% 6i]", -42, 42);
    COMPARE("precision=[%#.0o][%.0u][%08.4x]", 0U, 0U, 0x2aU);
    COMPARE("left=[%-9d] width=[%*u]", 12, -8, 31U);
    COMPARE("string=[%-8.3s] char=[%4c] %%", "abcdef", 'Q');
    COMPARE("hh=%hhd h=%hd l=%ld ll=%lld", (int)-7, (int)-1234, (long)-7654321,
            (long long)-1234567890123LL);
    COMPARE("hh=%hhu h=%hu z=%zu t=%td j=%jd", (int)250, (int)60000,
            (size_t)123456, (ptrdiff_t)-9876, (intmax_t)-1234567890123LL);
    COMPARE("hex=%llx upper=%#llX", (unsigned long long)0x123456789abcdef0ULL,
            (unsigned long long)0xabcdeULL);
    COMPARE("p=%p nullstr=%.0s", (void *)&n, "ignored");
    COMPARE("[%05%]");

    /* libc-compatible truncation and NUL termination, including size one. */
    memset(a, 'x', sizeof(a)); memset(b, 'x', sizeof(b));
    r = snprintf(a, 5, "%s", "abcdefgh");
    CHECK(r == aster_nv_snprintf(b, 5, "%s", "abcdefgh"));
    CHECK(strcmp(a, b) == 0 && b[4] == '\0');
    CHECK(aster_nv_snprintf(b, 1, "abc") == 3 && b[0] == '\0');
    CHECK(aster_nv_snprintf(NULL, 0, "%#x", 255U) == snprintf(NULL, 0, "%#x", 255U));
    CHECK(aster_nv_snprintf(NULL, 1, "x") == -1);

    /* Unsupported directives and pathological sizes fail closed. */
    CHECK(aster_nv_snprintf(b, sizeof(b), "bad %f", 1.0) == -1);
    CHECK(aster_nv_snprintf(b, sizeof(b), "bad %n", &n) == -1 && n == 77);
    CHECK(aster_nv_snprintf(b, sizeof(b), "%2147483647d", 1) == -1);
    CHECK(aster_nv_snprintf(b, sizeof(b), "%*d", 1048577, 1) == -1);
    CHECK(aster_nv_snprintf(b, sizeof(b), "%.*s", -1, "abc") == 3);

    memset(big, 'A', sizeof(big)); big[sizeof(big) - 1] = '\0';
    logged_length = 0;
    CHECK(nv_printf(3, "prefix:%s", big) == (int)(sizeof(big) + 6));
    CHECK(logged_length == 1023);
    CHECK(memcmp(logged + 1009, "...[truncated]", 14) == 0);
    CHECK(logged[logged_length - 1] == ']');
    memset(exact, 'B', sizeof(exact) - 1); exact[sizeof(exact) - 1] = '\0';
    logged_length = 0;
    CHECK(nv_printf(3, "%s", exact) == 1023);
    CHECK(logged_length == 1023 && logged[1022] == 'B');
    CHECK(memcmp(logged + 1009, "...[truncated]", 14) != 0);
    logged_length = 0;
    CHECK(nv_printf(1, "unsupported %f", 2.0) == -1);
    CHECK(logged_length == strlen("nvidia-nvport: <formatting failed>"));
    CHECK(memcmp(logged, "nvidia-nvport: <formatting failed>", logged_length) == 0);

    if (failures) {
        fprintf(stderr, "%d formatter test(s) failed\n", failures);
        return 1;
    }
    puts("diagnostics formatter tests passed");
    return 0;
}
