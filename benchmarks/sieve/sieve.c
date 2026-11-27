#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

static double now_ms(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (double)t.tv_sec * 1000.0 + (double)t.tv_nsec / 1e6;
}

static uint32_t count_primes(uint32_t limit) {
    uint8_t *composite = calloc((size_t)limit + 1u, sizeof(uint8_t));
    if (composite == NULL) abort();
    composite[0] = 1;
    composite[1] = 1;

    for (uint32_t p = 2; p * p <= limit; p++) {
        if (composite[p] == 0) {
            for (uint32_t multiple = p * p; multiple <= limit; multiple += p) {
                composite[multiple] = 1;
            }
        }
    }

    uint32_t count = 0;
    for (uint32_t candidate = 2; candidate <= limit; candidate++) {
        if (composite[candidate] == 0) count++;
    }
    free(composite);
    return count;
}

int main(void) {
    const double start = now_ms();
    uint32_t checksum = 0;
    for (uint32_t run = 0; run < 5; run++) {
        checksum += count_primes(UINT32_C(1000000) + run);
    }
    const double elapsed = now_ms() - start;
    printf("{\"checksum\":%" PRIu32 ",\"elapsedMs\":%.3f}\n", checksum, elapsed);
    return 0;
}
