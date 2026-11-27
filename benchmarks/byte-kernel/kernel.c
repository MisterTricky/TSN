#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <time.h>

static double now_ms(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (double)t.tv_sec * 1000.0 + (double)t.tv_nsec / 1e6;
}

static uint32_t kernel(uint32_t n) {
    uint32_t hash = UINT32_C(2166136261);
    uint32_t acc = 0;
    for (uint32_t i = 0; i < n; i++) {
        uint32_t byte = (i * 31u + (i >> 7u) + 17u) & 255u;
        hash = (hash ^ byte) * UINT32_C(16777619);
        acc += (byte * 13u + i) & 65535u;
    }
    return hash ^ acc;
}

int main(void) {
    const uint32_t n = UINT32_C(16777216);
    const uint32_t runs = 5;
    double start = now_ms();
    uint32_t checksum = 0;
    for (uint32_t run = 0; run < runs; run++) {
        checksum ^= kernel(n + run);
    }
    double elapsed = now_ms() - start;
    printf("{\"checksum\":%" PRIu32 ",\"elapsedMs\":%.3f}\n", checksum, elapsed);
    return 0;
}
