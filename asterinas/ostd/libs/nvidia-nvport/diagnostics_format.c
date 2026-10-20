// SPDX-License-Identifier: MPL-2.0
/* A small, freestanding printf subset for NVport diagnostics. */
#include <nvtypes.h>
#include <os-interface.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>

#define NV_FMT_LIMIT ((size_t)1048576)
#define LOG_BUFFER_SIZE 1024U
#define TRUNC_MARKER "...[truncated]"
#define FAIL_MARKER "nvidia-nvport: <formatting failed>"

extern void aster_nv_log_bytes(const char *bytes, NvU32 length);

typedef struct {
    char *buffer;
    size_t capacity;
    size_t length;
} nv_out;

typedef struct {
    unsigned left, plus, space, alternate, zero;
    size_t width, precision;
    unsigned precision_set;
} nv_spec;

enum nv_length { LEN_NONE, LEN_HH, LEN_H, LEN_L, LEN_LL, LEN_Z, LEN_T, LEN_J };

static int out_char(nv_out *o, char c)
{
    if (o->length >= NV_FMT_LIMIT) return -1;
    if (o->capacity && o->length < o->capacity - 1) o->buffer[o->length] = c;
    ++o->length;
    return 0;
}

static int out_repeat(nv_out *o, char c, size_t count)
{
    if (count > NV_FMT_LIMIT - o->length) return -1;
    while (count--) if (out_char(o, c) < 0) return -1;
    return 0;
}

static int parse_number(const char **p, size_t *value, size_t *scanned)
{
    size_t n = 0;
    while (**p >= '0' && **p <= '9') {
        if (++*scanned > NV_FMT_LIMIT) return -1;
        unsigned digit = (unsigned)(*(*p)++ - '0');
        if (n > (NV_FMT_LIMIT - digit) / 10) return -1;
        n = n * 10 + digit;
    }
    *value = n;
    return 0;
}

static uintmax_t get_unsigned(va_list *ap, enum nv_length len)
{
    switch (len) {
    case LEN_HH: return (unsigned char)va_arg(*ap, int);
    case LEN_H: return (unsigned short)va_arg(*ap, int);
    case LEN_L: return va_arg(*ap, unsigned long);
    case LEN_LL: return va_arg(*ap, unsigned long long);
    case LEN_Z: return va_arg(*ap, size_t);
    case LEN_T: return (uintmax_t)va_arg(*ap, size_t);
    case LEN_J: return va_arg(*ap, uintmax_t);
    default: return va_arg(*ap, unsigned int);
    }
}

static intmax_t get_signed(va_list *ap, enum nv_length len)
{
    switch (len) {
    case LEN_HH: return (signed char)va_arg(*ap, int);
    case LEN_H: return (short)va_arg(*ap, int);
    case LEN_L: return va_arg(*ap, long);
    case LEN_LL: return va_arg(*ap, long long);
    case LEN_Z: case LEN_T: return va_arg(*ap, ptrdiff_t);
    case LEN_J: return va_arg(*ap, intmax_t);
    default: return va_arg(*ap, int);
    }
}

static int emit_number(nv_out *o, uintmax_t value, int negative, unsigned base,
                       int upper, int pointer, const nv_spec *s)
{
    char reversed[sizeof(uintmax_t) * 8];
    size_t digits = 0, zeros, prefix = 0, pad;
    char sign = negative ? '-' : (s->plus ? '+' : (s->space ? ' ' : 0));
    const char *alphabet = upper ? "0123456789ABCDEF" : "0123456789abcdef";
    if (value || !s->precision_set || s->precision) {
        do { reversed[digits++] = alphabet[value % base]; value /= base; } while (value);
    }
    zeros = s->precision_set && s->precision > digits ? s->precision - digits : 0;
    if (pointer || (s->alternate && base == 16 && digits)) prefix = 2;
    if (s->alternate && base == 8 && (!digits || (!zeros && reversed[digits - 1] != '0'))) zeros++;
    if (pointer) sign = 0;
    pad = s->width > digits + zeros + prefix + (sign != 0) ?
          s->width - digits - zeros - prefix - (sign != 0) : 0;
    if (!s->left && (!s->zero || s->precision_set) && out_repeat(o, ' ', pad) < 0) return -1;
    if (sign && out_char(o, sign) < 0) return -1;
    if (prefix) {
        if (out_char(o, '0') < 0 || out_char(o, upper ? 'X' : 'x') < 0) return -1;
    }
    if (!s->left && s->zero && !s->precision_set && out_repeat(o, '0', pad) < 0) return -1;
    if (out_repeat(o, '0', zeros) < 0) return -1;
    while (digits) if (out_char(o, reversed[--digits]) < 0) return -1;
    if (s->left && out_repeat(o, ' ', pad) < 0) return -1;
    return 0;
}

static int emit_text(nv_out *o, const char *text, size_t length, const nv_spec *s)
{
    size_t pad = s->width > length ? s->width - length : 0;
    if (!s->left && out_repeat(o, ' ', pad) < 0) return -1;
    while (length--) if (out_char(o, *text++) < 0) return -1;
    if (s->left && out_repeat(o, ' ', pad) < 0) return -1;
    return 0;
}

static int format_core(char *buffer, NvU32 capacity, const char *fmt, va_list *args)
{
    nv_out out = { buffer, capacity, 0 };
    size_t scanned = 0;
    if (!fmt || (capacity && !buffer)) return -1;
    if (capacity) buffer[0] = '\0';
    while (*fmt) {
        nv_spec s = {0, 0, 0, 0, 0, 0, 0, 0};
        enum nv_length len = LEN_NONE;
        char conversion;
        if (++scanned > NV_FMT_LIMIT) return -1;
        if (*fmt != '%') {
            if (out_char(&out, *fmt++) < 0) return -1;
            continue;
        }
        ++fmt;
        if (++scanned > NV_FMT_LIMIT) return -1;
        for (;;) {
            if (*fmt == '-') s.left = 1;
            else if (*fmt == '+') s.plus = 1;
            else if (*fmt == ' ') s.space = 1;
            else if (*fmt == '#') s.alternate = 1;
            else if (*fmt == '0') s.zero = 1;
            else break;
            ++fmt; if (++scanned > NV_FMT_LIMIT) return -1;
        }
        if (*fmt == '*') {
            int w = va_arg(*args, int); ++fmt; ++scanned;
            if (w < 0) { s.left = 1; s.width = (size_t)(-(int64_t)w); }
            else s.width = (size_t)w;
            if (s.width > NV_FMT_LIMIT) return -1;
        } else if (parse_number(&fmt, &s.width, &scanned) < 0) return -1;
        if (s.width > NV_FMT_LIMIT) return -1;
        if (*fmt == '.') {
            ++fmt; ++scanned; s.precision_set = 1;
            if (*fmt == '*') {
                int p = va_arg(*args, int); ++fmt; ++scanned;
                if (p < 0) s.precision_set = 0;
                else s.precision = (size_t)p;
            } else if (parse_number(&fmt, &s.precision, &scanned) < 0) return -1;
            if (s.precision > NV_FMT_LIMIT) return -1;
        }
        if (fmt[0] == 'h' && fmt[1] == 'h') { len = LEN_HH; fmt += 2; }
        else if (*fmt == 'h') { len = LEN_H; ++fmt; }
        else if (fmt[0] == 'l' && fmt[1] == 'l') { len = LEN_LL; fmt += 2; }
        else if (*fmt == 'l') { len = LEN_L; ++fmt; }
        else if (*fmt == 'z') { len = LEN_Z; ++fmt; }
        else if (*fmt == 't') { len = LEN_T; ++fmt; }
        else if (*fmt == 'j') { len = LEN_J; ++fmt; }
        conversion = *fmt++;
        if (!conversion || ++scanned > NV_FMT_LIMIT) return -1;
        switch (conversion) {
        case 'd': case 'i': {
            intmax_t signed_value = get_signed(args, len);
            uintmax_t magnitude = signed_value < 0 ? (uintmax_t)(-(signed_value + 1)) + 1 : (uintmax_t)signed_value;
            if (emit_number(&out, magnitude, signed_value < 0, 10, 0, 0, &s) < 0) return -1;
            break;
        }
        case 'u': case 'o': case 'x': case 'X': {
            unsigned base = conversion == 'o' ? 8 : ((conversion == 'x' || conversion == 'X') ? 16 : 10);
            if (emit_number(&out, get_unsigned(args, len), 0, base, conversion == 'X', 0, &s) < 0) return -1;
            break;
        }
        case 'p': {
            void *ptr = va_arg(*args, void *);
            s.alternate = 1;
            if (emit_number(&out, (uintptr_t)ptr, 0, 16, 0, 1, &s) < 0) return -1;
            break;
        }
        case 's': {
            const char *str = va_arg(*args, const char *);
            size_t n = 0;
            if (!str) str = "(null)";
            while (n < (s.precision_set ? s.precision : NV_FMT_LIMIT) && str[n]) ++n;
            if (!s.precision_set && n == NV_FMT_LIMIT) return -1;
            if (emit_text(&out, str, n, &s) < 0) return -1;
            break;
        }
        case 'c': {
            char c = (char)va_arg(*args, int);
            if (emit_text(&out, &c, 1, &s) < 0) return -1;
            break;
        }
        case '%': {
            if (len != LEN_NONE || out_char(&out, '%') < 0) return -1;
            break;
        }
        default: return -1; /* Includes floating-point and the unsafe %n. */
        }
    }
    if (capacity) buffer[out.length < capacity ? out.length : capacity - 1] = '\0';
    return (int)out.length;
}

int aster_nv_snprintf(char *buffer, NvU32 capacity, const char *fmt, ...)
{
    va_list args;
    int result;
    va_start(args, fmt);
    result = format_core(buffer, capacity, fmt, &args);
    va_end(args);
    return result;
}

int nv_printf(NvU32 level, const char *fmt, ...)
{
    char buffer[LOG_BUFFER_SIZE];
    va_list args;
    int result;
    NvU32 length;
    (void)level; /* The platform logger currently accepts bytes, not a level. */
    va_start(args, fmt);
    result = format_core(buffer, sizeof(buffer), fmt, &args);
    va_end(args);
    if (result < 0) {
        static const char failure[] = FAIL_MARKER;
        aster_nv_log_bytes(failure, (NvU32)(sizeof(failure) - 1));
        return -1;
    }
    length = (NvU32)((size_t)result < sizeof(buffer) ? (size_t)result : sizeof(buffer) - 1);
    if ((size_t)result > sizeof(buffer) - 1) {
        const char marker[] = TRUNC_MARKER;
        const size_t marker_len = sizeof(marker) - 1;
        const size_t keep = sizeof(buffer) - 1 - marker_len;
        for (size_t i = 0; i < marker_len; ++i) buffer[keep + i] = marker[i];
        buffer[sizeof(buffer) - 1] = '\0';
        length = sizeof(buffer) - 1;
    }
    aster_nv_log_bytes(buffer, length);
    return result;
}
