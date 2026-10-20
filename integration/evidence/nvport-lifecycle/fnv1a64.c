/* CPU-only, noncryptographic readback checksum. Not firmware authentication. */
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
int main(int argc, char **argv)
{
    if (argc != 2) return 2;
    FILE *file = fopen(argv[1], "rb");
    if (!file) return 2;
    uint64_t hash = UINT64_C(0xcbf29ce484222325);
    unsigned char bytes[65536];
    size_t count;
    while ((count = fread(bytes, 1, sizeof(bytes), file)) != 0)
        for (size_t i = 0; i < count; ++i)
            hash = (hash ^ bytes[i]) * UINT64_C(0x100000001b3);
    if (ferror(file) || fclose(file)) return 2;
    printf("%016" PRIx64 "\n", hash);
    return 0;
}
