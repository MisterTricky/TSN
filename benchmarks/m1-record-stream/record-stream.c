#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

enum { RECORDS = 100000, RECORD_BYTES = 1000, PAYLOAD_BYTES = 996 };

int main(void) {
    uint32_t hash = UINT32_C(2166136261);
    uint64_t bytes = 0;
    uint8_t record[PAYLOAD_BYTES];

    for (uint32_t r = 0; r < RECORDS; ++r) {
        uint8_t prefix[4];
        if (fread(prefix, 1, sizeof prefix, stdin) != sizeof prefix) return 2;
        uint32_t length = (uint32_t)prefix[0] | ((uint32_t)prefix[1] << 8) |
                          ((uint32_t)prefix[2] << 16) | ((uint32_t)prefix[3] << 24);
        if (length != PAYLOAD_BYTES || fread(record, 1, length, stdin) != length) return 3;
        int valid = 1;
        for (uint32_t i = 0; i < length; ++i) {
            uint8_t expected = (uint8_t)(r * 17u + i * 31u + 0x5au);
            if (record[i] != expected) valid = 0;
            hash = (hash ^ record[i]) * UINT32_C(16777619);
        }
        if (!valid) return 4;
        bytes += 4u + length;
    }

    if (fgetc(stdin) != EOF) return 5;
    printf("%u\n%llu\n%u\n", RECORDS, (unsigned long long)bytes, hash);
    return 0;
}
